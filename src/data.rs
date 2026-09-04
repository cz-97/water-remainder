use crate::ui::now;
use std::{env, fs, path::PathBuf};
fn time_file() -> PathBuf {
    let base = env::var_os("APPDATA")
        .or_else(|| env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("water-remainder").join("data.txt")
}

pub fn load_timestamps() -> Vec<u64> {
    let mut timestamps = Vec::new();
    if let Ok(text) = fs::read_to_string(time_file()) {
        for line in text.lines() {
            if let Ok(d) = line.parse() {
                timestamps.push(d);
            }
        }
    }
    timestamps
}

pub fn get_last_time() -> Option<u64> {
    load_timestamps().last().copied()
}

pub fn save_time() {
    let p = time_file();
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let mut timestamps = load_timestamps();
    timestamps.push(now());
    let _ = fs::write(
        p,
        timestamps
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
