pub mod main_window;
pub mod reminder_window;
pub mod settings_window;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use gpui_kit::{ImageFormat, rgb};
use std::time::{SystemTime, UNIX_EPOCH};

/// 界面配色唯一来源。`gpui_kit::rgb()` 不是 `const fn`，无法定义 `const Rgba`，
/// 因此这里存原始 `u32`，使用处统一写 `rgb(palette::ACCENT)`。
pub mod palette {
    // 窗口外壳
    pub const WINDOW_BG: u32 = 0x1f1f1f;
    pub const ROW_HOVER: u32 = 0x292929;
    pub const WHITE: u32 = 0xffffff;

    // 文字：由强到弱
    pub const TEXT: u32 = 0xe5e7eb;
    pub const TEXT_MUTED: u32 = 0x94a3b8;
    pub const TEXT_SOFT: u32 = 0xcbd5e1;
    pub const TEXT_DISABLED: u32 = 0x64748b;

    // 交互
    pub const ACCENT: u32 = 0x60a5fa;

    /// 日历热力图色阶，索引即等级（0 = 无记录），由浅到深。
    pub const CALENDAR: [u32; 9] = [
        0x404040, 0xdbeafe, 0xbfdbfe, 0x93c5fd, 0x60a5fa, 0x3b82f6, 0x2563eb, 0x1d4ed8, 0x1e3a8a,
    ];

    /// 浅色热力图格子上的深色文字（让日期数字在浅蓝底上仍可读）。
    pub const CALENDAR_TEXT_DARK: u32 = 0x0f172a;

    /// 提醒浮层的半透明遮罩（此处是 hsla，不是 rgb）。
    pub fn overlay_bg() -> gpui_kit::Hsla {
        gpui_kit::hsla(0., 0., 0., 0.85)
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn local_date(timestamp: u64) -> NaiveDate {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|d| d.with_timezone(&Local).date_naive())
        .unwrap_or_else(|| Local::now().date_naive())
}
/// 本地时区下某一天的秒区间 `[起, 止)`，用于把「某天明细」变成一次索引区间查询。
/// 与 `local_date()` 共用同一套时区规则，因此区间内的记录与「按时间戳判定的本地日期」严格一致。
/// 只有本地零点不存在的时区（DST 会跳过零点的少数地区）才会返回 `None`，由调用方降级处理。
pub fn day_bounds(day: NaiveDate) -> Option<(u64, u64)> {
    Some((local_midnight(day)?, local_midnight(day.succ_opt()?)?))
}

fn local_midnight(day: NaiveDate) -> Option<u64> {
    day.and_hms_opt(0, 0, 0)?
        .and_local_timezone(Local)
        .earliest()
        .map(|dt| dt.timestamp() as u64)
}

pub fn format_clock(timestamp: u64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|d| d.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".into())
}
/// 带秒的时钟，用于提醒浮层里括号内的「具体时间」。
pub fn format_clock_secs(timestamp: u64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|d| d.with_timezone(&Local).format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".into())
}
/// 把秒数写成「1 天 2 小时 15 分 30 秒」：从最大非零单位一路展开到秒，
/// 保证文本每秒都会变化，同时不会出现「1 小时 59 秒」这种有歧义的省略写法。
pub fn format_span(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (hours, rest) = (rest / 3_600, rest % 3_600);
    let (minutes, secs) = (rest / 60, rest % 60);

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{} 天", days));
        parts.push(format!("{} 小时", hours));
        parts.push(format!("{} 分", minutes));
    } else if hours > 0 {
        parts.push(format!("{} 小时", hours));
        parts.push(format!("{} 分", minutes));
    } else if minutes > 0 {
        parts.push(format!("{} 分", minutes));
    }
    parts.push(format!("{} 秒", secs));
    parts.join(" ")
}
/// 相对日期前缀：当天为空串，之后是「昨天 」/「前天 」/「N 天前 」。
pub fn format_day_label(day: NaiveDate, today: NaiveDate) -> String {
    match today.signed_duration_since(day).num_days() {
        days if days <= 0 => String::new(),
        1 => "昨天 ".into(),
        2 => "前天 ".into(),
        days => format!("{} 天前 ", days),
    }
}
pub fn relative_to_now(timestamp: u64) -> String {
    let seconds = now().saturating_sub(timestamp);
    if seconds < 60 {
        "刚刚".into()
    } else if seconds < 3600 {
        format!("{} 分钟前", seconds / 60)
    } else {
        format!("{} 小时前", seconds / 3600)
    }
}
pub fn format_date(date: NaiveDate) -> String {
    format!("{}年{}月{}日", date.year(), date.month(), date.day())
}

/// 次数 → 热力图等级：每 3 次升一级并封顶，0 次单独一档。
pub fn calendar_level(count: usize) -> usize {
    count.div_ceil(3).min(palette::CALENDAR.len() - 1)
}

/// 次数 → 热力图取色，等级由 `calendar_level` 给出。
pub fn calendar_color(count: usize) -> gpui_kit::Rgba {
    rgb(palette::CALENDAR[calendar_level(count)])
}

/// 按文件头魔数判断图片格式，供自选提醒图片解码（不信任文件扩展名）。
pub fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
    let starts = |signature: &[u8]| bytes.starts_with(signature);
    if starts(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        Some(ImageFormat::Png)
    } else if starts(&[0xff, 0xd8, 0xff]) {
        Some(ImageFormat::Jpeg)
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some(ImageFormat::Gif)
    } else if starts(b"BM") {
        Some(ImageFormat::Bmp)
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(&b"WEBP"[..]) {
        Some(ImageFormat::Webp)
    } else if starts(&[0x49, 0x49, 0x2a, 0x00]) || starts(&[0x4d, 0x4d, 0x00, 0x2a]) {
        Some(ImageFormat::Tiff)
    } else if starts(&[0x00, 0x00, 0x01, 0x00]) {
        Some(ImageFormat::Ico)
    } else {
        None
    }
}
