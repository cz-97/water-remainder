use crate::ui::now;
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::{
    env, fs,
    path::PathBuf,
    sync::{OnceLock, RwLock},
};

type DbPool = Pool<SqliteConnectionManager>;

static DB_POOL: OnceLock<DbPool> = OnceLock::new();

static LAST_TIME: OnceLock<RwLock<Option<u64>>> = OnceLock::new();

fn last_time_cache() -> &'static RwLock<Option<u64>> {
    LAST_TIME.get_or_init(|| RwLock::new(None))
}

fn data_file() -> PathBuf {
    let base = env::var_os("APPDATA")
        .or_else(|| env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    base.join("water-remainder").join("data.db")
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

pub fn load_timestamps() -> Vec<u64> {
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

pub fn get_elapsed() -> Option<u64> {
    let mut time = *last_time_cache().read().unwrap();

    if time.is_none() {
        time = get_conn()?
            .prepare(
                "SELECT timestamp
                FROM drink_records
                ORDER BY timestamp DESC
                LIMIT 1",
            )
            .ok()?
            .query_row([], |row| row.get::<_, i64>(0))
            .ok()
            .map(|v| v.max(0) as u64);

        *last_time_cache().write().unwrap() = time;
    }

    time.map(|time| now().saturating_sub(time))
}

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
        *last_time_cache().write().unwrap() = Some(timestamp);
    }
}
