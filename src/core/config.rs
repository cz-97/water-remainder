use crate::core::paths::app_dir;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// 最小间隔（秒）。调度线程也用它兜底，防止 0 造成紧循环。
pub const MIN_INTERVAL: u64 = 60;
pub const DEFAULT_INTERVAL: u64 = 45 * MIN_INTERVAL;

/// 把任意来源的间隔收敛到最接近的合法档位，避免非法间隔造成紧循环或设置窗显示错档；
/// 正好落在两档中间时取下界。
pub fn normalize_interval(seconds: u64) -> u64 {
    INTERVALS
        .iter()
        .copied()
        .min_by_key(|candidate| candidate.abs_diff(seconds))
        .unwrap_or(DEFAULT_INTERVAL)
}

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

/// 界面主题以小写字符串写入 TOML，便于阅读和手动编辑。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// 跟随系统深浅（默认）。
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    /// 设置窗里作为**预览卡片**展示的两个取值（跟随系统是开关，不是卡片）。
    pub const CHOICES: [Theme; 2] = [Theme::Light, Theme::Dark];

    /// 界面上的中文名。
    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "跟随系统",
            Theme::Light => "浅色",
            Theme::Dark => "深色",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(deserialize_with = "deserialize_or_default")]
    pub interval_secs: u64,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub autostart: bool,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_secs: DEFAULT_INTERVAL,
            autostart: false,
            theme: Theme::System,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub settings: Settings,
    #[serde(rename = "window", skip_serializing_if = "Option::is_none")]
    pub window_state: Option<WindowState>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    #[serde(deserialize_with = "deserialize_or_default")]
    pub x: f32,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub y: f32,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub width: f32,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub height: f32,
    #[serde(deserialize_with = "deserialize_or_default")]
    pub maximized: bool,
}

fn deserialize_or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(T::deserialize(deserializer).unwrap_or_default())
}

fn settings_file() -> PathBuf {
    app_dir().join("settings.toml")
}

pub fn load_store() -> Result<Store, String> {
    let text = match fs::read_to_string(settings_file()) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Store::default());
        }
        Err(error) => return Err(format!("读取设置文件失败：{error}")),
    };
    parse_store(&text).map_err(|error| format!("解析设置文件失败：{error}"))
}

/// 解析 TOML 配置并收敛间隔。缺失或类型无效的字段使用各自默认值；语法错误返回错误。
fn parse_store(text: &str) -> Result<Store, toml::de::Error> {
    let mut s = toml::from_str::<Store>(text)?;
    s.settings.interval_secs = normalize_interval(s.settings.interval_secs);
    Ok(s)
}
/// 取一份设置快照。
///
/// 锁中毒（此前有线程持锁时 panic）时仍然把数据读出来：`Settings` 是纯数据，
/// 中毒只是「有人 panic 过」的标记，不代表内容损坏。这里刻意不 `unwrap` ——
/// release 下 `panic = "abort"`，在启动路径上 panic 会让进程连同托盘图标一起
/// 无声消失，而这正是本项目一直在消除的那类结局。
pub fn settings_snapshot(store: &Arc<Mutex<Store>>) -> Settings {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .settings
        .clone()
}

/// 落盘设置与窗口状态。
///
/// 返回写入结果而不是内部吞掉：磁盘满、目录只读、被安全软件锁住都会失败，
/// 而调用方（设置窗）需要据此决定要不要把界面改回原样 —— 让开关显示「已开启」
/// 而文件里其实没写进去，就是在骗用户。
pub fn save_store(s: &Store) -> std::io::Result<()> {
    let p = settings_file();
    if let Some(d) = p.parent() {
        fs::create_dir_all(d)?;
    }
    let out = toml::to_string_pretty(s).map_err(std::io::Error::other)?;
    write_atomic(&p, &out)
}

/// 原子地覆盖写 TOML 文件：先写同目录下的临时文件并 `sync_all`，再改名覆盖。
///
/// 直接 `fs::write` 会在写入途中把原文件截断；本程序启动时又无条件按 `autostart`
/// 调 [`crate::platform::set_autostart`]，所以一次「写到一半就崩溃」会让
/// 配置回退成默认值（`autostart = false`），进而**静默关掉用户的开机启动**。
/// 改名在 Windows 上对已存在的目标文件是覆盖式原子替换，读到的永远是完整的旧内容
/// 或完整的新内容。
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        // 改名失败时别把临时文件留在数据目录里。
        let _ = fs::remove_file(&tmp);
    })
}

/// 改设置的唯一入口：加锁、修改、落盘都在这里完成，调用方只描述「改什么」。
///
/// 落盘失败时**把内存改回去**再返回错误 —— 否则界面显示的是「已生效」，而
/// 配置文件里还是旧值，用户重启一次就会发现设置莫名复原。宁可当场
/// 告诉用户没存住，也不要制造这种分叉。
pub fn update_settings<R>(
    store: &Arc<Mutex<Store>>,
    edit: impl FnOnce(&mut Settings) -> R,
) -> Result<R, SaveError> {
    update_settings_with(store, edit, save_store)
}

/// `update_settings` 的实现主体，落盘动作可注入。
///
/// 之所以把保存步骤当参数传进来，是为了能测试「落盘失败时内存值被回滚」这条
/// 关键路径 —— 否则测试就得去写用户真实的配置文件，那是绝不能做的事。
fn update_settings_with<R>(
    store: &Arc<Mutex<Store>>,
    edit: impl FnOnce(&mut Settings) -> R,
    save: impl FnOnce(&Store) -> std::io::Result<()>,
) -> Result<R, SaveError> {
    let mut guard = store.lock().map_err(|_| SaveError::Locked)?;
    let before = guard.settings.clone();
    let result = edit(&mut guard.settings);
    match save(&guard) {
        Ok(()) => Ok(result),
        Err(error) => {
            guard.settings = before;
            Err(SaveError::Io(error))
        }
    }
}

/// 改设置失败的原因。分成两种是为了让界面能说清楚：锁中毒是程序内部异常，
/// 写文件失败则通常意味着磁盘满或目录权限问题。
#[derive(Debug)]
pub enum SaveError {
    /// `Mutex` 中毒：此前某个持锁线程 panic 过。
    Locked,
    Io(std::io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Locked => write!(f, "设置状态异常"),
            SaveError::Io(error) => write!(f, "写入设置文件失败：{error}"),
        }
    }
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: 0.,
            y: 0.,
            width: 600.,
            height: 800.,
            maximized: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_interval_handles_edges_and_nearest_bucket() {
        for &seconds in INTERVALS {
            assert_eq!(normalize_interval(seconds), seconds);
        }
        assert_eq!(normalize_interval(0), INTERVALS[0]);
        assert_eq!(normalize_interval(u64::MAX), *INTERVALS.last().unwrap());
        assert_eq!(normalize_interval(INTERVALS[2] + 1), INTERVALS[2]);
        assert_eq!(normalize_interval(INTERVALS[2] - 1), INTERVALS[2]);

        let (lower, upper) = (INTERVALS[3], INTERVALS[4]);
        assert_eq!(normalize_interval((lower + upper) / 2), lower);
    }

    #[test]
    fn toml_defaults_missing_values_and_normalizes_interval() {
        let store = parse_store("[settings]\nautostart = true\ninterval_secs = 0\n").unwrap();
        assert_eq!(store.settings.interval_secs, INTERVALS[0]);
        assert!(store.settings.autostart);
        assert_eq!(store.settings.theme, Theme::System);
        assert!(store.window_state.is_none());

        let defaults = parse_store("").unwrap();
        assert_eq!(defaults.settings.interval_secs, DEFAULT_INTERVAL);
        assert!(!defaults.settings.autostart);
    }

    #[test]
    fn toml_loads_settings_and_window_state() {
        let store = parse_store(
            "[settings]\ninterval_secs = 2700\nautostart = true\ntheme = \"dark\"\n\n[window]\nx = 100.0\ny = 200.0\nwidth = 640.0\nheight = 900.0\nmaximized = true\n",
        )
        .unwrap();
        assert_eq!(store.settings.interval_secs, 2700);
        assert!(store.settings.autostart);
        assert_eq!(store.settings.theme, Theme::Dark);
        let window = store.window_state.unwrap();
        assert_eq!(window.x, 100.);
        assert_eq!(window.y, 200.);
        assert_eq!(window.width, 640.);
        assert_eq!(window.height, 900.);
        assert!(window.maximized);
    }

    #[test]
    fn invalid_toml_is_reported_and_invalid_fields_fall_back() {
        assert!(parse_store("[settings\ninterval_secs = 2700\n").is_err());
        let store = parse_store(
            "[settings]\ninterval_secs = 2700\nautostart = \"yes\"\ntheme = \"unknown\"\n",
        )
        .unwrap();
        assert_eq!(store.settings.interval_secs, 2700);
        assert!(!store.settings.autostart);
        assert_eq!(store.settings.theme, Theme::System);
    }

    #[test]
    fn store_round_trips_through_toml() {
        let store = Store {
            settings: Settings {
                interval_secs: 2700,
                autostart: true,
                theme: Theme::Dark,
            },
            window_state: Some(WindowState {
                x: 100.,
                y: 200.,
                width: 640.,
                height: 900.,
                maximized: true,
            }),
        };
        let encoded = toml::to_string_pretty(&store).unwrap();
        assert!(encoded.contains("[settings]"));
        assert!(encoded.contains("theme = \"dark\""));
        let decoded = parse_store(&encoded).unwrap();
        assert_eq!(decoded.settings.interval_secs, 2700);
        assert!(decoded.settings.autostart);
        assert_eq!(decoded.settings.theme, Theme::Dark);
        let window = decoded.window_state.unwrap();
        assert_eq!(window.width, 640.);
        assert!(window.maximized);
    }

    #[test]
    fn write_atomic_replaces_contents_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("wr-config-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.toml");

        write_atomic(&path, "[settings]\ninterval_secs = 2700\n").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[settings]\ninterval_secs = 2700\n"
        );
        write_atomic(&path, "[settings]\nautostart = true\n").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[settings]\nautostart = true\n"
        );

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "settings.toml")
            .collect();
        assert!(leftovers.is_empty(), "目录里残留了临时文件：{leftovers:?}");
        fs::remove_dir_all(&dir).ok();
    }

    fn store_with(interval: u64, autostart: bool) -> Arc<Mutex<Store>> {
        Arc::new(Mutex::new(Store {
            settings: Settings {
                interval_secs: interval,
                autostart,
                theme: Theme::System,
            },
            window_state: None,
        }))
    }

    #[test]
    fn update_settings_applies_and_reports_success() {
        let store = store_with(3000, false);
        let result = update_settings_with(
            &store,
            |settings| {
                settings.autostart = true;
                settings.autostart
            },
            |_| Ok(()),
        );
        assert!(result.unwrap());
        assert!(store.lock().unwrap().settings.autostart);
    }

    #[test]
    fn update_settings_rolls_back_when_saving_fails() {
        let store = store_with(3000, false);
        let result = update_settings_with(
            &store,
            |settings| {
                settings.interval_secs = 900;
                settings.autostart = true;
            },
            |_| Err(std::io::Error::other("disk full")),
        );
        assert!(result.is_err());
        let after = store.lock().unwrap().settings.clone();
        assert_eq!(after.interval_secs, 3000);
        assert!(!after.autostart);
    }

    #[test]
    fn settings_snapshot_reads_a_poisoned_lock() {
        let store = store_with(2400, true);
        let clone = store.clone();
        let _ = std::thread::spawn(move || {
            let _guard = clone.lock().unwrap();
            panic!("simulate panic while holding the lock");
        })
        .join();
        assert!(store.lock().is_err());
        let settings = settings_snapshot(&store);
        assert_eq!(settings.interval_secs, 2400);
        assert!(settings.autostart);
    }
}
