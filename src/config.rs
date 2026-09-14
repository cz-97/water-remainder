use crate::paths::app_dir;
use std::{fs, path::PathBuf};

const MIN_INTERVAL: u64 = 60;
pub const DEFAULT_INTERVAL: u64 = 45 * MIN_INTERVAL;

/// 可选提醒间隔（秒），设置窗与调度器共用这一份来源。
pub const INTERVALS: &[u64] = &[
    15 * MIN_INTERVAL,
    20 * MIN_INTERVAL,
    25 * MIN_INTERVAL,
    30 * MIN_INTERVAL,
    35 * MIN_INTERVAL,
    40 * MIN_INTERVAL,
    45 * MIN_INTERVAL,
    50 * MIN_INTERVAL,
    55 * MIN_INTERVAL,
    60 * MIN_INTERVAL,
    65 * MIN_INTERVAL,
    70 * MIN_INTERVAL,
    75 * MIN_INTERVAL,
];

#[derive(Clone)]
pub struct Settings {
    pub interval_secs: u64,
    pub autostart: bool,
}
pub struct Store {
    pub settings: Settings,
    pub window_state: Option<WindowState>,
}
#[derive(Clone, Copy)]
pub struct WindowState {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

fn settings_file() -> PathBuf {
    app_dir().join("settings.txt")
}

pub fn load_store() -> Store {
    let mut s = Store {
        settings: Settings {
            interval_secs: DEFAULT_INTERVAL,
            autostart: false,
        },
        window_state: None,
    };
    if let Ok(text) = fs::read_to_string(settings_file()) {
        for line in text.lines() {
            let mut p = line.splitn(2, '=');
            match (p.next(), p.next()) {
                (Some("interval"), Some(v)) => {
                    s.settings.interval_secs = v.parse().unwrap_or(DEFAULT_INTERVAL)
                }
                (Some("autostart"), Some(v)) => s.settings.autostart = v == "true",
                (Some("window_x"), Some(v)) => {
                    s.window_state.get_or_insert_with(default_window_state).x =
                        v.parse().unwrap_or(0.)
                }
                (Some("window_y"), Some(v)) => {
                    s.window_state.get_or_insert_with(default_window_state).y =
                        v.parse().unwrap_or(0.)
                }
                (Some("window_width"), Some(v)) => {
                    s.window_state
                        .get_or_insert_with(default_window_state)
                        .width = v.parse().unwrap_or(600.)
                }
                (Some("window_height"), Some(v)) => {
                    s.window_state
                        .get_or_insert_with(default_window_state)
                        .height = v.parse().unwrap_or(800.)
                }
                (Some("window_maximized"), Some(v)) => {
                    s.window_state
                        .get_or_insert_with(default_window_state)
                        .maximized = v == "true"
                }
                _ => {}
            }
        }
    }
    s
}
pub fn save_store(s: &Store) {
    let p = settings_file();
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let mut out = format!(
        "interval={}\nautostart={}\n",
        s.settings.interval_secs, s.settings.autostart
    );
    if let Some(w) = s.window_state {
        out.push_str(&format!(
            "window_x={}\nwindow_y={}\nwindow_width={}\nwindow_height={}\nwindow_maximized={}\n",
            w.x, w.y, w.width, w.height, w.maximized
        ));
    }
    let _ = fs::write(p, out);
}

fn default_window_state() -> WindowState {
    WindowState {
        x: 0.,
        y: 0.,
        width: 600.,
        height: 800.,
        maximized: false,
    }
}
