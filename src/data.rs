use crate::ui::now;
use rusqlite::{Connection, params};
use std::{env, fs, path::PathBuf};

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
         CREATE TABLE IF NOT EXISTS drink_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp INTEGER NOT NULL
         );",
    )
    .ok()?;
    Some(conn)
}


pub fn load_timestamps() -> Vec<u64> {
    let Some(conn) = open_db() else {
        return Vec::new();
    };

    let mut stmt = match conn.prepare("SELECT timestamp FROM drink_records ORDER BY timestamp ASC") {
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
    load_timestamps().last().copied()
}

pub fn save_time() {
    let Some(conn) = open_db() else {
        return;
    };
    let timestamp = now() as i64;
    let _ = conn.execute(
        "INSERT INTO drink_records (timestamp) VALUES (?1)",
        params![timestamp],
    );
}
