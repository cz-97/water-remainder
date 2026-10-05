use crate::core::paths::app_dir;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// 最小间隔（秒）。调度线程也用它兜底，防止 0 造成紧循环。
pub const MIN_INTERVAL: u64 = 60;
pub const DEFAULT_INTERVAL: u64 = 45 * MIN_INTERVAL;

/// 把任意来源的间隔收敛到最接近的合法档位。
///
/// `settings.txt` 是纯文本、可以手改，旧版本也可能留下表外的值。不收敛会有两个后果：
/// `0` 让调度线程的 `recv_timeout(0)` 立刻超时、deadline 又永远已过，于是退化成
/// 每轮发一次提醒的紧循环；表外值让设置窗的步进器 `position(..)` 落到索引 0，
/// 显示错误分档，点一下就把原值覆盖成表里的第二档。正好落在两档中间时取下界。
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

#[derive(Clone)]
pub struct Settings {
    pub interval_secs: u64,
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_secs: DEFAULT_INTERVAL,
            autostart: false,
        }
    }
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
    match fs::read_to_string(settings_file()) {
        Ok(text) => parse_store(&text),
        // 文件不存在（首次启动）或读不出来：全部走默认值。
        Err(_) => parse_store(""),
    }
}

/// 把 `settings.txt` 的文本解析成 `Store`。
///
/// 抽成纯函数是为了能直接对「损坏时逐行回退默认值」这条约定写测试：文件是纯文本、
/// 用户可手改，所以未知键要忽略、解析失败的单个值只影响它自己那一项，不能连累
/// 同一文件里的其它设置。
fn parse_store(text: &str) -> Store {
    let mut s = Store {
        settings: Settings::default(),
        window_state: None,
    };
    for line in text.lines() {
        let mut p = line.splitn(2, '=');
        match (p.next(), p.next()) {
            (Some("interval"), Some(v)) => {
                // 解析失败与越界都收敛到最近的合法档位，见 `normalize_interval`。
                s.settings.interval_secs =
                    normalize_interval(v.trim().parse().unwrap_or(DEFAULT_INTERVAL))
            }
            (Some("autostart"), Some(v)) => s.settings.autostart = v == "true",
            (Some("window_x"), Some(v)) => {
                s.window_state.get_or_insert_with(default_window_state).x = v.parse().unwrap_or(0.)
            }
            (Some("window_y"), Some(v)) => {
                s.window_state.get_or_insert_with(default_window_state).y = v.parse().unwrap_or(0.)
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
    s
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
    write_atomic(&p, &out)
}

/// 原子地覆盖写一个小文本文件：先写同目录下的临时文件并 `sync_all`，再改名覆盖。
///
/// 直接 `fs::write` 会在写入途中把原文件截断；本程序启动时又无条件按 `autostart`
/// 调 [`crate::platform::set_autostart`]，所以一次「写到一半就崩溃」会让
/// `settings.txt` 回退成默认值（`autostart = false`），进而**静默关掉用户的开机启动**。
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
/// `settings.txt` 里还是旧值，用户重启一次就会发现设置莫名复原。宁可当场
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
/// 关键路径 —— 否则测试就得去写用户真实的 `settings.txt`，那是绝不能做的事。
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

fn default_window_state() -> WindowState {
    WindowState {
        x: 0.,
        y: 0.,
        width: 600.,
        height: 800.,
        maximized: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `settings.txt` 可手改，非法间隔必须被收敛到表内档位，否则 0 会让调度线程
    /// 退化成每秒发一次提醒的紧循环，表外值会让设置窗步进器显示错档。
    #[test]
    fn normalize_interval_accepts_table_values_unchanged() {
        for &seconds in INTERVALS {
            assert_eq!(normalize_interval(seconds), seconds);
        }
    }

    #[test]
    fn normalize_interval_collapses_zero_and_absurd_values() {
        assert_eq!(normalize_interval(0), INTERVALS[0]);
        assert_eq!(normalize_interval(u64::MAX), *INTERVALS.last().unwrap());
    }

    /// 正好落在两档中间时取下界 —— 文档明确写了这条，且它是任意的但必须稳定。
    #[test]
    fn normalize_interval_breaks_ties_downward() {
        let (lower, upper) = (INTERVALS[3], INTERVALS[4]);
        let midpoint = (lower + upper) / 2;
        assert_eq!(
            midpoint - lower,
            upper - midpoint,
            "构造用例时两档必须等距，否则测不到平局"
        );
        assert_eq!(normalize_interval(midpoint), lower);
    }

    #[test]
    fn normalize_interval_picks_the_nearest_bucket() {
        assert_eq!(normalize_interval(INTERVALS[2] + 1), INTERVALS[2]);
        assert_eq!(normalize_interval(INTERVALS[2] - 1), INTERVALS[2]);
    }

    #[test]
    fn parse_store_defaults_on_empty_text() {
        let store = parse_store("");
        assert_eq!(store.settings.interval_secs, DEFAULT_INTERVAL);
        assert!(!store.settings.autostart);
        assert!(store.window_state.is_none());
    }

    /// 损坏的值只影响它自己那一项，同一文件里的其它设置必须保住 ——
    /// 否则一次手改失误会把用户的全部设置清空。
    #[test]
    fn parse_store_keeps_good_keys_when_one_value_is_corrupt() {
        let store = parse_store("interval=not-a-number\nautostart=true\n");
        assert_eq!(store.settings.interval_secs, DEFAULT_INTERVAL);
        assert!(store.settings.autostart, "坏值不能连累 autostart");
    }

    #[test]
    fn parse_store_ignores_unknown_and_malformed_lines() {
        let store = parse_store("garbage\n=5\nunknown=1\ninterval=2700\n");
        assert_eq!(store.settings.interval_secs, 2700);
    }

    #[test]
    fn parse_store_reads_window_state_and_maximized_flag() {
        let store = parse_store(
            "window_x=100\nwindow_y=200\nwindow_width=640\nwindow_height=900\nwindow_maximized=true\n",
        );
        let window = store.window_state.expect("应解析出窗口状态");
        assert_eq!(window.x, 100.);
        assert_eq!(window.y, 200.);
        assert_eq!(window.width, 640.);
        assert_eq!(window.height, 900.);
        assert!(window.maximized);
    }

    /// `autostart` 只认字面量 `true`：写 `1` / `True` 不能被当成开启，
    /// 否则启动时会把注册表 Run 键设成与设置显示不一致的状态。
    #[test]
    fn parse_store_treats_only_literal_true_as_autostart_on() {
        for text in [
            "autostart=1",
            "autostart=True",
            "autostart=yes",
            "autostart=",
        ] {
            assert!(
                !parse_store(text).settings.autostart,
                "{text:?} 不应被当成开启"
            );
        }
        assert!(parse_store("autostart=true").settings.autostart);
    }

    #[test]
    fn write_atomic_replaces_contents_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("wr-config-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.txt");

        write_atomic(&path, "first-version-longer").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first-version-longer");
        // 覆盖写一个更短的内容：旧内容不能有残留。
        write_atomic(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "settings.txt")
            .collect();
        assert!(leftovers.is_empty(), "目录里残留了临时文件：{leftovers:?}");

        fs::remove_dir_all(&dir).ok();
    }

    fn store_with(interval: u64, autostart: bool) -> Arc<Mutex<Store>> {
        Arc::new(Mutex::new(Store {
            settings: Settings {
                interval_secs: interval,
                autostart,
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

    /// 落盘失败必须把内存值改回去。否则界面显示「已开启」而文件里仍是旧值，
    /// 用户重启一次就会发现设置莫名复原 —— 这是刻意避免的分叉。
    #[test]
    fn update_settings_rolls_back_when_saving_fails() {
        let store = store_with(3000, false);
        let result = update_settings_with(
            &store,
            |settings| {
                settings.interval_secs = 900;
                settings.autostart = true;
            },
            |_| Err(std::io::Error::other("磁盘满了")),
        );
        assert!(result.is_err(), "落盘失败必须返回错误");
        let after = store.lock().unwrap().settings.clone();
        assert_eq!(after.interval_secs, 3000, "间隔必须回滚");
        assert!(!after.autostart, "开机启动必须回滚");
    }

    /// 返回的错误要能说清原因，界面才有东西可显示。
    #[test]
    fn save_error_describes_the_failure() {
        let error = SaveError::Io(std::io::Error::other("磁盘满了"));
        assert!(error.to_string().contains("磁盘满了"));
        assert!(!SaveError::Locked.to_string().is_empty());
    }

    /// 锁中毒（有线程持锁时 panic）不能让程序崩，也不能把好数据丢掉：
    /// release 下 `panic = "abort"`，启动路径上 panic 会无声退出。
    #[test]
    fn settings_snapshot_survives_a_poisoned_lock() {
        let store = store_with(2400, true);
        let clone = store.clone();
        // 在持锁状态下 panic，把 Mutex 变成中毒状态。
        let _ = std::thread::spawn(move || {
            let _guard = clone.lock().unwrap();
            panic!("模拟持锁线程 panic");
        })
        .join();
        assert!(store.lock().is_err(), "前提：锁应已中毒");

        let settings = settings_snapshot(&store);
        assert_eq!(settings.interval_secs, 2400);
        assert!(settings.autostart);
    }
}
