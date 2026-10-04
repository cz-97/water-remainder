use std::{env, path::PathBuf};

/// 应用数据目录 `%APPDATA%\water-remainder`（回退到 `LOCALAPPDATA`，再回退到当前目录）。
/// 设置文件与数据库都落在这里，避免两处各推导一遍。
pub fn app_dir() -> PathBuf {
    env::var_os("APPDATA")
        .or_else(|| env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("water-remainder")
}

/// 自定义提醒图片的落盘位置：数据目录下的固定文件。存在即代表「已自定义」，
/// 因此不需要在设置里记录任何路径。
pub fn reminder_image_file() -> PathBuf {
    app_dir().join("reminder.img")
}
