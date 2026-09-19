use crate::paths::app_dir;
use crate::ui::{day_bounds, local_date, now};
use chrono::NaiveDate;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, params_from_iter, types::Value};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock},
};

type DbPool = Pool<SqliteConnectionManager>;

static DB_POOL: OnceLock<DbPool> = OnceLock::new();

/// 天级聚合缓存（日期 → 次数 / 当天最后一次）。进程内只从数据库读一次，之后仅由 save_time 增量维护。
static COUNTS: OnceLock<RwLock<Option<Arc<DayCounts>>>> = OnceLock::new();

/// 明细缓存：只放「查过的日期」。空天靠次数短路，永远不会进这里。
static DETAILS: OnceLock<RwLock<BTreeMap<NaiveDate, Arc<Vec<u64>>>>> = OnceLock::new();

fn counts_slot() -> &'static RwLock<Option<Arc<DayCounts>>> {
    COUNTS.get_or_init(|| RwLock::new(None))
}

fn details_slot() -> &'static RwLock<BTreeMap<NaiveDate, Arc<Vec<u64>>>> {
    DETAILS.get_or_init(|| RwLock::new(BTreeMap::new()))
}

/// 某一天的聚合结果。`last` 是那天最后喝水的时刻，使「上次喝水」不必读明细。
#[derive(Clone, Copy)]
struct DayStat {
    count: usize,
    last: u64,
}

/// 日期 → 聚合结果，key 升序。规模只与「有记录的天数」成正比，与明细行数无关。
/// `Clone` 是 `Arc::make_mut`（save_time 增量维护）的前提。
#[derive(Clone, Default)]
pub struct DayCounts {
    days: BTreeMap<NaiveDate, DayStat>,
}

impl DayCounts {
    fn from_rows(rows: impl IntoIterator<Item = (NaiveDate, usize, u64)>) -> Self {
        Self {
            days: rows
                .into_iter()
                .map(|(day, count, last)| (day, DayStat { count, last }))
                .collect(),
        }
    }

    /// 某天喝了多少次。
    pub fn count(&self, day: NaiveDate) -> usize {
        self.days.get(&day).map_or(0, |stat| stat.count)
    }

    /// 最早有记录的那一天。
    pub fn earliest_day(&self) -> Option<NaiveDate> {
        self.days.keys().next().copied()
    }

    /// 全部记录中最近的一次。key 升序、且每天存的是当日最大值，所以最后一个分组即全局最近。
    pub fn last_time(&self) -> Option<u64> {
        self.days.values().next_back().map(|stat| stat.last)
    }

    /// 记一次新喝水：只动这一天的两个数字，不碰明细。
    fn record(&mut self, timestamp: u64) {
        self.days
            .entry(local_date(timestamp))
            .and_modify(|stat| {
                stat.count += 1;
                stat.last = stat.last.max(timestamp);
            })
            .or_insert(DayStat {
                count: 1,
                last: timestamp,
            });
    }
}

fn data_file() -> PathBuf {
    app_dir().join("data.db")
}

fn init_database(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA busy_timeout = 5000;
         PRAGMA journal_mode = WAL;

         CREATE TABLE IF NOT EXISTS drink_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL
         );

         CREATE INDEX IF NOT EXISTS idx_timestamp
         ON drink_records(timestamp);",
    )
}

fn db_pool() -> &'static DbPool {
    DB_POOL.get_or_init(|| {
        let path = data_file();

        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).expect("failed to create database directory");
        }

        // 先创建临时连接
        let conn = rusqlite::Connection::open(&path).expect("failed to open database");

        // 初始化数据库
        init_database(&conn).expect("failed to initialize database");

        // 初始化完成后关闭临时连接
        drop(conn);

        // 再创建连接池
        let manager = SqliteConnectionManager::file(path);

        Pool::builder()
            .max_size(4)
            .min_idle(Some(1))
            .build(manager)
            .expect("failed to create database connection pool")
    })
}

fn get_conn() -> Option<PooledConnection<SqliteConnectionManager>> {
    db_pool().get().ok()
}

/// 唯一一次全量读库：交给 SQLite 按本地日期聚合，只回「天 / 次数 / 当天最后一次」，不回明细。
/// 一天一行，因此结果与明细行数无关——这正是日历只需要条数的原因。
fn query_day_counts() -> Vec<(NaiveDate, usize, u64)> {
    let Some(conn) = get_conn() else {
        return Vec::new();
    };

    let mut stmt = match conn.prepare(
        "SELECT date(timestamp, 'unixepoch', 'localtime') AS day,
                COUNT(*),
                MAX(timestamp)
         FROM drink_records
         GROUP BY day
         ORDER BY day ASC",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    }) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    rows.filter_map(|row| row.ok())
        .filter_map(|(day, count, last)| {
            Some((
                NaiveDate::parse_from_str(&day, "%Y-%m-%d").ok()?,
                count.max(0) as usize,
                last.max(0) as u64,
            ))
        })
        .collect()
}

/// 取某天的全部明细（升序）。按本地日的秒区间过滤，命中 idx_timestamp 做索引查找；
/// 只有本地零点不存在时（DST 跳过零点）才退回按日期函数过滤。
fn query_day(day: NaiveDate) -> Vec<u64> {
    let Some(conn) = get_conn() else {
        return Vec::new();
    };

    let (sql, bound): (&str, Vec<Value>) = match day_bounds(day) {
        Some((start, end)) => (
            "SELECT timestamp
             FROM drink_records
             WHERE timestamp >= ?1 AND timestamp < ?2
             ORDER BY timestamp ASC",
            vec![Value::Integer(start as i64), Value::Integer(end as i64)],
        ),
        None => (
            "SELECT timestamp
             FROM drink_records
             WHERE date(timestamp, 'unixepoch', 'localtime') = ?1
             ORDER BY timestamp ASC",
            vec![Value::Text(day.format("%Y-%m-%d").to_string())],
        ),
    };

    let mut stmt = match conn.prepare(sql) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map(params_from_iter(bound), |row| row.get::<_, i64>(0)) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    rows.filter_map(|row| row.ok().map(|value| value.max(0) as u64))
        .collect()
}

/// 天级聚合缓存。首次调用时做唯一一次聚合查询，之后永远命中内存。
pub fn day_counts() -> Arc<DayCounts> {
    if let Some(counts) = counts_slot().read().unwrap().as_ref() {
        return counts.clone();
    }

    // 首帧之前先在写锁内完成加载，保证全进程只产生这一次 SELECT。
    let mut slot = counts_slot().write().unwrap();
    if slot.is_none() {
        *slot = Some(Arc::new(DayCounts::from_rows(query_day_counts())));
    }
    slot.as_ref().unwrap().clone()
}

/// 某天的明细。次数为 0 直接返回空、不查库；查过一次后就命中缓存。
pub fn day_detail(day: NaiveDate) -> Arc<Vec<u64>> {
    if day_counts().count(day) == 0 {
        return Arc::default();
    }

    if let Some(times) = details_slot().read().unwrap().get(&day).cloned() {
        return times;
    }

    let times = Arc::new(query_day(day));
    // 并发下同一天可能被查两次，两份内容等价，保留先写入的那份即可。
    details_slot()
        .write()
        .unwrap()
        .entry(day)
        .or_insert_with(|| times.clone());
    times
}

/// 全部记录中最近的一次喝水时间戳。
pub fn last_time() -> Option<u64> {
    day_counts().last_time()
}

/// 最近一次喝水距今的秒数。直接从聚合缓存取，不需要明细。
pub fn get_elapsed() -> Option<u64> {
    last_time().map(|time| now().saturating_sub(time))
}

/// 落库一次喝水记录，并同步进内存缓存（不触发任何回读）。
pub fn save_time() {
    let Some(conn) = get_conn() else {
        return;
    };

    let timestamp = now();

    if conn
        .execute(
            "INSERT INTO drink_records (timestamp) VALUES (?1)",
            params![timestamp as i64],
        )
        .is_err()
    {
        return;
    }

    // 天级：改动这一天的两个数字即可。缓存尚未建立时无需处理：之后的聚合查询会包含这条记录。
    if let Some(counts) = counts_slot().write().unwrap().as_mut() {
        Arc::make_mut(counts).record(timestamp);
    }

    // 明细：只在那天的明细已经加载过时才追加，否则留到点击时一次查回（不做任何 「顺带加载」）。
    let day = local_date(timestamp);
    if let Some(times) = details_slot().write().unwrap().get_mut(&day) {
        let times = Arc::make_mut(times);
        match times.last() {
            Some(&last) if last > timestamp => {
                let pos = times.partition_point(|&t| t <= timestamp);
                times.insert(pos, timestamp);
            }
            _ => times.push(timestamp),
        }
    }
}
