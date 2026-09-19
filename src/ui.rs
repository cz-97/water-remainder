use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use gpui_kit::{Div, FontWeight, Stateful, WindowControlArea, div, prelude::*, px, rgb};
use std::time::{SystemTime, UNIX_EPOCH};

/// 界面配色唯一来源。`gpui_kit::rgb()` 不是 `const fn`，无法定义 `const Rgba`，
/// 因此这里存原始 `u32`，使用处统一写 `rgb(palette::ACCENT)`。
pub mod palette {
    // 窗口外壳
    pub const WINDOW_BG: u32 = 0x1f1f1f;
    pub const TITLEBAR_BG: u32 = 0x1d1d1d;
    pub const TITLEBAR_FG: u32 = 0xe0f2fe;
    pub const TITLEBAR_HOVER: u32 = 0x254c77;
    pub const ROW_HOVER: u32 = 0x292929;
    pub const CLOSE_HOVER: u32 = 0xdc2626;
    pub const WHITE: u32 = 0xffffff;

    // 文字：由强到弱
    pub const TEXT: u32 = 0xe5e7eb;
    pub const TEXT_MUTED: u32 = 0x94a3b8;
    pub const TEXT_SOFT: u32 = 0xcbd5e1;
    pub const TEXT_BUTTON: u32 = 0xe2e8f0;
    pub const TEXT_DISABLED: u32 = 0x64748b;

    // 交互
    pub const ACCENT: u32 = 0x60a5fa;
    pub const ACCENT_HOVER: u32 = 0x3b82f6;
    pub const BORDER: u32 = 0x64748b;
    pub const SWITCH_OFF: u32 = 0x475569;
    pub const STEP_BG: u32 = 0x334155;
    pub const STEP_BG_HOVER: u32 = 0x475569;
    pub const STEP_BG_DISABLED: u32 = 0x252b33;

    /// 日历热力图色阶，索引即等级（0 = 无记录），由浅到深。
    pub const CALENDAR: [u32; 9] = [
        0x404040, 0xdbeafe, 0xbfdbfe, 0x93c5fd, 0x60a5fa, 0x3b82f6, 0x2563eb, 0x1d4ed8, 0x1e3a8a,
    ];

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
pub fn calendar_color(count: usize) -> gpui_kit::Rgba {
    let level = count.div_ceil(3).min(palette::CALENDAR.len() - 1);
    rgb(palette::CALENDAR[level])
}

/// 自绘标题栏高度。栏内按钮与它同高，所以两者共用一个常量。
const TITLEBAR_HEIGHT: f32 = 38.;
/// 标题栏按钮宽度：窗口控制键与功能键统一 46px。
const TITLEBAR_BUTTON_WIDTH: f32 = 46.;
/// 标题栏图标字体（Windows 自带）。
const ICON_FONT: &str = "Segoe Fluent Icons";
/// 窗口控制键（最小化 / 最大化 / 关闭）的图标字号。
const WINDOW_ICON_SIZE: f32 = 12.;
/// 标题栏功能键（设置）的图标字号。
pub const ACTION_ICON_SIZE: f32 = 14.;

/// 两个窗口共用的自绘标题栏：左侧可拖拽的标题，右侧由调用方传入按钮组。
pub fn titlebar(title: &'static str, actions: impl IntoElement) -> impl IntoElement {
    div()
        .h(px(TITLEBAR_HEIGHT))
        .flex()
        .items_center()
        .bg(rgb(palette::TITLEBAR_BG))
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .px_4()
                .text_color(rgb(palette::TITLEBAR_FG))
                .font_weight(FontWeight::BOLD)
                .window_control_area(WindowControlArea::Drag)
                .child(title),
        )
        .child(actions)
}

/// 标题栏内图标按钮的**唯一**样式来源：46×38、图标居中、悬停换底色。
/// 窗口控制键与设置键都基于它，改外观只需改这一处。
///
/// - `id`：gpui 的元素状态标识，按钮之间必须唯一。
/// - `hover`：悬停底色，必须在参数里给出 —— gpui 的 `InteractiveElement::hover`
///   对同一元素二次调用会 panic，调用方不能再自己补一个 `.hover(..)`。
/// - `icon_size`：图标字号，取 `WINDOW_ICON_SIZE` 或 `ACTION_ICON_SIZE`。
///
/// 返回具体的 `Stateful<Div>`，调用方可以继续链式追加自己的交互
/// （`.cursor_pointer()` / `.occlude()` / `.on_mouse_down(..)` …）。
pub fn titlebar_button(id: &'static str, hover: u32, icon_size: f32) -> Stateful<Div> {
    div()
        .id(id)
        .font_family(ICON_FONT)
        .w(px(TITLEBAR_BUTTON_WIDTH))
        .h(px(TITLEBAR_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(icon_size))
        .text_color(rgb(palette::TITLEBAR_FG))
        .hover(move |s| s.bg(rgb(hover)))
}

/// 窗口控制键（最小化 / 最大化 / 关闭）：共用样式之上附加系统控制区语义。
pub fn window_button(label: &'static str, area: WindowControlArea) -> impl IntoElement {
    let hover = if area == WindowControlArea::Close {
        palette::CLOSE_HOVER
    } else {
        palette::TITLEBAR_HOVER
    };
    let id = match area {
        WindowControlArea::Min => "window-minimize",
        WindowControlArea::Max => "window-maximize",
        WindowControlArea::Close => "window-close",
        WindowControlArea::Drag => "window-drag",
    };
    titlebar_button(id, hover, WINDOW_ICON_SIZE)
        .occlude()
        .window_control_area(area)
        .child(label)
}
