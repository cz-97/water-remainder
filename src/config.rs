use std::{env, fs, path::PathBuf};
const MIN_INTERVAL: u64 = 60;
pub const DEFAULT_INTERVAL: u64 = 60 * MIN_INTERVAL;
macro_rules! intervals {
    ($($m:expr),*) => {
        &[
            $(
                ($m * MIN_INTERVAL, concat!($m, " 分钟")),
            )*
        ]
    };
}
pub const INTERVALS: &[(u64, &str)] = intervals![15, 30, 45, 60, 75, 90];

#[derive(Clone)]
pub struct Settings {
    pub interval_secs: u64,
    pub autostart: bool,
    pub startup_remind: bool,
}
pub struct Store {
    pub settings: Settings,
    pub drinks: Vec<u64>,
}
fn data_file() -> PathBuf {
    let base = env::var_os("APPDATA")
        .or_else(|| env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("water-remainder").join("data.txt")
}
pub fn load_store() -> Store {
    let mut s = Store {
        settings: Settings {
            interval_secs: DEFAULT_INTERVAL,
            autostart: false,
            startup_remind: true,
        },
        drinks: Vec::new(),
    };
    if let Ok(text) = fs::read_to_string(data_file()) {
        for line in text.lines() {
            let mut p = line.splitn(2, '=');
            match (p.next(), p.next()) {
                (Some("interval"), Some(v)) => {
                    s.settings.interval_secs = v.parse().unwrap_or(DEFAULT_INTERVAL).max(60)
                }
                (Some("autostart"), Some(v)) => s.settings.autostart = v == "true",
                (Some("startup_remind"), Some(v)) => s.settings.startup_remind = v == "true",
                (Some("drink"), Some(v)) => {
                    if let Ok(t) = v.parse() {
                        s.drinks.push(t)
                    }
                }
                _ => {}
            }
        }
    }
    s
}
pub fn save_store(s: &Store) {
    let p = data_file();
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let mut out = format!(
        "interval={}\nautostart={}\nstartup_remind={}\n",
        s.settings.interval_secs, s.settings.autostart, s.settings.startup_remind
    );
    for t in &s.drinks {
        out.push_str(&format!("drink={}\n", t));
    }
    let _ = fs::write(p, out);
}
