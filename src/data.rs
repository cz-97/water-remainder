use crate::paths::app_dir;
use crate::ui::{local_date, now};
use chrono::NaiveDate;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock},
};

type DbPool = Pool<SqliteConnectionManager>;

static DB_POOL: OnceLock<DbPool> = OnceLock::new();

/// 全量记录缓存。进程生命周期内只从数据库读取一次，之后仅由 save_time 增量维护。
static RECORDS: OnceLock<RwLock<Option<Arc<DrinkCache>>>> = OnceLock::new();

fn records_slot() -> &'static RwLock<Option<Arc<DrinkCache>>> {
    RECORDS.get_or_init(|| RwLock::new(None))
}

/// 按本地日期分桶的喝水记录，桶内时间戳升序。
#[derive(Clone, Default)]
pub struct DrinkCache {
    by_day: BTreeMap<NaiveDate, Vec<u64>>,
}

impl DrinkCache {
    fn from_timestamps(timestamps: impl IntoIterator<Item = u64>) -> Self {
        let mut all: Vec<u64> = timestamps.into_iter().collect();
        all.sort_unstable();

        let mut by_day: BTreeMap<NaiveDate, Vec<u64>> = BTreeMap::new();
        for timestamp in all {
            by_day
                .entry(local_date(timestamp))
                .or_default()
                .push(timestamp);
        }
        Self { by_day }
    }

    /// 某天喝了多少次。
    pub fn count(&self, day: NaiveDate) -> usize {
        self.by_day.get(&day).map_or(0, Vec::len)
    }

    /// 某天的全部记录（升序）。
    pub fn times(&self, day: NaiveDate) -> &[u64] {
        self.by_day.get(&day).map_or(&[], Vec::as_slice)
    }

    /// 最早有记录的那一天。
    pub fn earliest_day(&self) -> Option<NaiveDate> {
        self.by_day.keys().next().copied()
    }

    /// 全部记录中最近的一次。
    pub fn last_time(&self) -> Option<u64> {
        self.by_day
            .values()
            .next_back()
            .and_then(|bucket| bucket.last().copied())
    }

    /// 把新记录放进所属日期的桶里，保持桶内升序（正常时钟下即尾部追加）。
    fn insert(&mut self, timestamp: u64) {
        let bucket = self.by_day.entry(local_date(timestamp)).or_default();
        match bucket.last() {
            Some(&last) if last > timestamp => {
                let pos = bucket.partition_point(|&t| t <= timestamp);
                bucket.insert(pos, timestamp);
            }
            _ => bucket.push(timestamp),
        }
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

/// 唯一一次全量读库。
fn query_all_timestamps() -> Vec<u64> {
    let Some(conn) = get_conn() else {
        return Vec::new();
    };

    let mut stmt = match conn.prepare(
        "SELECT timestamp
         FROM drink_records
         ORDER BY timestamp ASC",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map([], |row| row.get::<_, i64>(0)) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    rows.filter_map(|row| row.ok().map(|value| value.max(0) as u64))
        .collect()
}

/// 取记录缓存。首次调用时全量加载数据库，之后永远命中内存。
pub fn snapshot() -> Arc<DrinkCache> {
    if let Some(cache) = records_slot().read().unwrap().as_ref() {
        return cache.clone();
    }

    // 首帧之前先在写锁内完成加载，保证全进程只产生这一次 SELECT。
    let mut slot = records_slot().write().unwrap();
    if slot.is_none() {
        *slot = Some(Arc::new(
            DrinkCache::from_timestamps(query_all_timestamps()),
        ));
    }
    slot.as_ref().unwrap().clone()
}

/// 全部记录中最近的一次喝水时间戳。
pub fn last_time() -> Option<u64> {
    snapshot().last_time()
}

/// 最近一次喝水距今的秒数。直接从记录缓存取，不再单独维护一份「上次时间」。
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
        .is_ok()
    {
        // 缓存尚未建立时无需处理：之后的首次全量加载会包含这条记录。
        if let Some(cache) = records_slot().write().unwrap().as_mut() {
            Arc::make_mut(cache).insert(timestamp);
        }
    }
}
