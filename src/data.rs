use crate::ui::now;
use rusqlite::{Connection, params};
use std::sync::{OnceLock, RwLock};
use std::{env, fs, path::PathBuf};

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

fn open_db() -> Option<Connection> {
    let path = data_file();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let conn = Connection::open(path).ok()?;
    conn.execute_batch(
        "PRAGMA busy_timeout = 5000;
         PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS drink_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_timestamp ON drink_records(timestamp);",
    )
    .ok()?;
    Some(conn)
}

pub fn load_timestamps() -> Vec<u64> {
    let Some(conn) = open_db() else {
        return Vec::new();
    };

    let mut stmt = match conn.prepare("SELECT timestamp FROM drink_records ORDER BY timestamp ASC")
    {
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

pub fn get_last_time() -> Option<u64> {
    if let Some(time) = *last_time_cache().read().unwrap() {
        return Some(time);
    }

    let Some(conn) = open_db() else {
        return None;
    };

    let mut stmt = conn
        .prepare("SELECT timestamp FROM drink_records ORDER BY timestamp DESC LIMIT 1")
        .ok()?;

    let time = stmt
        .query_row([], |row| row.get::<_, i64>(0))
        .ok()
        .map(|v| v.max(0) as u64);

    *last_time_cache().write().unwrap() = time;

    time
}

pub fn save_time() {
    let Some(conn) = open_db() else {
        return;
    };

    let timestamp = now() as i64;

    if conn
        .execute(
            "INSERT INTO drink_records (timestamp) VALUES (?1)",
            params![timestamp],
        )
        .is_ok()
    {
        *last_time_cache().write().unwrap() = Some(timestamp as u64);
    }
}
