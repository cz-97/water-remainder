use crate::core::paths::app_dir;
use crate::ui::{day_bounds, local_date, now};
use chrono::{Datelike, Months, NaiveDate};
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

/// 连接池。`None` 表示存储不可用（建库或建池失败），此时所有读写都安全退化为空结果，
/// 而不是让整个进程消失。用 `OnceLock` 缓存结论，失败不重试。
static DB_POOL: OnceLock<Option<DbPool>> = OnceLock::new();

/// 按月缓存的日级聚合：key 为当月 1 号。未访问过的月份不查库；查过的月份（含空月）永久命中内存。
static MONTHS: OnceLock<RwLock<BTreeMap<NaiveDate, Arc<DayCounts>>>> = OnceLock::new();

/// 明细缓存：只放「查过的日期」。空天靠次数短路，永远不会进这里。
static DETAILS: OnceLock<RwLock<BTreeMap<NaiveDate, Arc<Vec<u64>>>>> = OnceLock::new();

/// 全局时间戳范围。日历只按需读当月，而左箭头边界与「上次喝水」需要全局极值，
/// 因此单独用一次走 `idx_timestamp` 索引的 MIN/MAX 查询维护，与按月聚合解耦。
static BOUNDS: OnceLock<RwLock<Option<Bounds>>> = OnceLock::new();

fn months_slot() -> &'static RwLock<BTreeMap<NaiveDate, Arc<DayCounts>>> {
    MONTHS.get_or_init(|| RwLock::new(BTreeMap::new()))
}

fn details_slot() -> &'static RwLock<BTreeMap<NaiveDate, Arc<Vec<u64>>>> {
    DETAILS.get_or_init(|| RwLock::new(BTreeMap::new()))
}

fn bounds_slot() -> &'static RwLock<Option<Bounds>> {
    BOUNDS.get_or_init(|| RwLock::new(None))
}

/// 全局最早 / 最近一次记录的时间戳；空库时两者均为 None。
#[derive(Clone, Copy)]
struct Bounds {
    min: Option<u64>,
    max: Option<u64>,
}

/// 一个自然月内「有记录的天 → 次数」，key 升序。规模只与当月有记录的天数成正比。
/// `Clone` 是 `Arc::make_mut`（save_time 增量维护）的前提。
#[derive(Clone, Default)]
pub struct DayCounts {
    days: BTreeMap<NaiveDate, usize>,
}

impl DayCounts {
    fn from_rows(rows: impl IntoIterator<Item = (NaiveDate, usize)>) -> Self {
        Self {
            days: rows.into_iter().collect(),
        }
    }

    /// 某天喝了多少次。
    pub fn count(&self, day: NaiveDate) -> usize {
        self.days.get(&day).copied().unwrap_or(0)
    }

    /// 记一次新喝水：只动这一天的计数。
    fn record(&mut self, timestamp: u64) {
        *self.days.entry(local_date(timestamp)).or_insert(0) += 1;
    }
}

fn data_file() -> PathBuf {
    app_dir().join("data.db")
}

fn init_database(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;

         CREATE TABLE IF NOT EXISTS drink_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL
         );

         CREATE INDEX IF NOT EXISTS idx_timestamp
         ON drink_records(timestamp);",
    )
}

/// 数据库的首次初始化。失败时返回 `false` 而不是 panic：本程序是
/// `windows_subsystem = "windows"` 的托盘程序（release 下 `panic = "abort"`），
/// panic 的表现是进程连同托盘图标一起无声消失，用户拿不到任何线索。
/// 数据文件损坏、被其它程序独占、目录无权限都会走到这里。
fn initialize(path: &std::path::Path) -> bool {
    if let Some(dir) = path.parent()
        && fs::create_dir_all(dir).is_err()
    {
        return false;
    }

    let Ok(conn) = rusqlite::Connection::open(path) else {
        return false;
    };
    if init_database(&conn).is_err() {
        return false;
    }
    // 临时连接用完即弃：连接池里的连接由 `with_init` 各自补 PRAGMA。
    drop(conn);
    true
}

fn db_pool() -> Option<&'static DbPool> {
    DB_POOL
        .get_or_init(|| {
            let path = data_file();
            if !initialize(&path) {
                return None;
            }

            // `journal_mode` 是写进文件头的持久设置，只需建库时设一次；`busy_timeout`
            // 与 `synchronous` 是**每连接**的，必须在 `with_init` 里逐条设置 ——
            // 只写在初始化用的临时连接上对池内连接毫无作用。
            let manager = SqliteConnectionManager::file(path).with_init(|conn| {
                conn.execute_batch(
                    "PRAGMA busy_timeout = 5000;
                     PRAGMA synchronous = NORMAL;",
                )
            });

            Pool::builder()
                .max_size(4)
                .min_idle(Some(1))
                .build(manager)
                .ok()
        })
        .as_ref()
}

fn get_conn() -> Option<PooledConnection<SqliteConnectionManager>> {
    db_pool()?.get().ok()
}

/// 单个自然月的日级聚合：一天一行，与明细行数无关。优先按本地零点区间过滤，
/// 命中 `idx_timestamp` 做索引范围扫描；只有本地零点不存在的时区（DST 跳过零点）才退回按月字符串过滤。
fn query_month_counts(month: NaiveDate) -> Vec<(NaiveDate, usize)> {
    let Some(conn) = get_conn() else {
        return Vec::new();
    };
    let Some(next_month) = month.checked_add_months(Months::new(1)) else {
        return Vec::new();
    };

    let (condition, bound): (&str, Vec<Value>) =
        match (day_bounds(month), day_bounds(next_month)) {
            (Some((start, _)), Some((end, _))) => (
                "timestamp >= ?1 AND timestamp < ?2",
                vec![Value::Integer(start as i64), Value::Integer(end as i64)],
            ),
            _ => (
                "strftime('%Y-%m', timestamp, 'unixepoch', 'localtime') = ?1",
                vec![Value::Text(month.format("%Y-%m").to_string())],
            ),
        };

    let sql = format!(
        "SELECT date(timestamp, 'unixepoch', 'localtime') AS day,
                COUNT(*)
         FROM drink_records
         WHERE {condition}
         GROUP BY day
         ORDER BY day ASC"
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map(params_from_iter(bound), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    }) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    rows.filter_map(|row| row.ok())
        .filter_map(|(day, count)| {
            Some((
                NaiveDate::parse_from_str(&day, "%Y-%m-%d").ok()?,
                count.max(0) as usize,
            ))
        })
        .collect()
}

/// 全局时间戳范围。走 `idx_timestamp` 索引的 MIN/MAX，代价与记录数无关。
fn query_bounds() -> Bounds {
    let empty = Bounds {
        min: None,
        max: None,
    };
    let Some(conn) = get_conn() else {
        return empty;
    };
    let mut stmt = match conn.prepare("SELECT MIN(timestamp), MAX(timestamp) FROM drink_records") {
        Ok(stmt) => stmt,
        Err(_) => return empty,
    };
    match stmt.query_row([], |row| {
        Ok((
            row.get::<_, Option<i64>>(0)?,
            row.get::<_, Option<i64>>(1)?,
        ))
    }) {
        Ok((min, max)) => Bounds {
            min: min.map(|value| value.max(0) as u64),
            max: max.map(|value| value.max(0) as u64),
        },
        Err(_) => empty,
    }
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

/// 某个月（传入该月任意一天）的天级聚合。首次访问该月时按索引区间查一次库，
/// 之后（含空月）永久命中内存 —— 日历切月因此只在首次产生一次 SELECT。
///
/// 查询刻意留在锁外（先查、后插入）：持写锁跑 SQL 会把日历每帧读聚合、
/// 调度线程读极值全部阻塞在一个索引扫描上。
pub fn month_counts(day_in_month: NaiveDate) -> Arc<DayCounts> {
    let month = day_in_month.with_day(1).unwrap_or(day_in_month);
    if let Some(counts) = months_slot().read().unwrap().get(&month).cloned() {
        return counts;
    }

    let counts = Arc::new(DayCounts::from_rows(query_month_counts(month)));
    // 并发下同一个月可能被查两次，两份内容等价，保留先写入的那份即可。
    months_slot()
        .write()
        .unwrap()
        .entry(month)
        .or_insert_with(|| counts.clone())
        .clone()
}

/// 某天的明细。该天次数为 0 直接返回空、不查库；查过一次后就命中缓存。
pub fn day_detail(day: NaiveDate) -> Arc<Vec<u64>> {
    if month_counts(day).count(day) == 0 {
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

/// 全局时间戳范围。首次调用做一次索引 MIN/MAX 查询，之后命中内存。
/// 与 `month_counts` 同理，查询留在锁外。
fn bounds() -> Bounds {
    if let Some(bounds) = *bounds_slot().read().unwrap() {
        return bounds;
    }

    let queried = query_bounds();
    *bounds_slot().write().unwrap().get_or_insert(queried)
}

/// 全部记录中最近的一次喝水时间戳。
pub fn last_time() -> Option<u64> {
    bounds().max
}

/// 最早有记录的那一天。
pub fn earliest_day() -> Option<NaiveDate> {
    bounds().min.map(local_date)
}

/// 最近一次喝水距今的秒数。直接从全局极值取，不需要明细。
pub fn get_elapsed() -> Option<u64> {
    last_time().map(|time| now().saturating_sub(time))
}

/// 落库一次喝水记录，并同步进内存缓存（不触发任何回读）。
///
/// 返回是否真的写成功。失败时**不更新任何缓存** —— 否则内存里会多出一条数据库里
/// 并不存在的记录，界面显示与落盘内容就此分叉；调用方（提醒浮层）也必须据此决定
/// 是否顺延提醒，把丢失的打卡当成成功会让下一次提醒按错误的时间点排期。
pub fn save_time() -> bool {
    let Some(conn) = get_conn() else {
        return false;
    };

    let timestamp = now();

    if conn
        .execute(
            "INSERT INTO drink_records (timestamp) VALUES (?1)",
            params![timestamp as i64],
        )
        .is_err()
    {
        return false;
    }

    let day = local_date(timestamp);
    let month = day.with_day(1).unwrap_or(day);

    // 天级：只在该月已加载时增量更新；未加载则留待首次访问时一次查回（已含这条记录）。
    if let Some(counts) = months_slot().write().unwrap().get_mut(&month) {
        Arc::make_mut(counts).record(timestamp);
    }

    // 全局极值：已加载时增量更新；未加载则留待首次查询（那次查询会包含这条记录）。
    if let Some(bounds) = bounds_slot().write().unwrap().as_mut() {
        bounds.min = Some(bounds.min.map_or(timestamp, |min| min.min(timestamp)));
        bounds.max = Some(bounds.max.map_or(timestamp, |max| max.max(timestamp)));
    }

    // 明细：只在那天的明细已经加载过时才追加，否则留到点击时一次查回（不做任何「顺带加载」）。
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

    true
}
