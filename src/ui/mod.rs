pub mod main_window;
pub mod reminder_window;
pub mod settings_window;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use gpui_kit::{
    Div, FontWeight, ImageFormat, Stateful, Window, WindowControlArea, div, prelude::*, px, rgb,
};
use std::time::{SystemTime, UNIX_EPOCH};

/// 界面配色唯一来源。深浅两套随**系统外观**切换（跟随系统深浅，不是强调色）：
/// [`palette::set_appearance`] 由 [`follow_system_appearance`] 在窗口创建时写入一次、
/// 之后每次系统切换深浅再写一次，下面所有取色函数读的都是当前这一份。
///
/// 全局只存一个外观值就够：颜色只在 GPUI 主线程上被读（每次 `render`）与写
/// （外观观察者回调），`AtomicU8` 既无锁也不会撕裂。
pub mod palette {
    use gpui_kit::WindowAppearance;
    use std::sync::atomic::{AtomicU8, Ordering};

    /// 深色外观：窗口壳比标题栏略亮，强调色偏亮以配深底。
    const DARK: Palette = Palette {
        window_bg: 0x1f1f1f,
        titlebar_bg: 0x1d1d1d,
        titlebar_fg: 0xe0e0e0,
        titlebar_hover: 0x254c77,
        row_hover: 0x292929,
        text: 0xe5e7eb,
        text_muted: 0x94a3b8,
        text_soft: 0xcbd5e1,
        text_button: 0xe2e8f0,
        text_disabled: 0x64748b,
        accent: 0x60a5fa,
        accent_hover: 0x3b82f6,
        switch_off: 0x475569,
        step_bg: 0x334155,
        step_bg_hover: 0x475569,
        step_bg_disabled: 0x252b33,
        // 0 次记录的日历格底色。
        calendar_base: 0x404040,
        // 0 次记录的日历格上的日期数字（弱化，不与有记录的格子争）。
        calendar_base_text: 0xcbd5e1,
        // 选中日的外框。
        selection_ring: 0xffffff,
    };

    /// 浅色外观：窗口壳纯白、标题栏略灰；强调色整体加深一档，
    /// 否则白字按钮在白底上读不出来。选中框改用深色 —— 白框会融进浅蓝格。
    const LIGHT: Palette = Palette {
        window_bg: 0xffffff,
        titlebar_bg: 0xf3f4f6,
        titlebar_fg: 0x1f2937,
        titlebar_hover: 0xbfdbfe,
        row_hover: 0xf3f4f6,
        text: 0x1f2937,
        text_muted: 0x64748b,
        text_soft: 0x475569,
        text_button: 0x334155,
        text_disabled: 0x94a3b8,
        accent: 0x2563eb,
        accent_hover: 0x1d4ed8,
        switch_off: 0xcbd5e1,
        step_bg: 0xe2e8f0,
        step_bg_hover: 0xcbd5e1,
        step_bg_disabled: 0xf1f5f9,
        calendar_base: 0xe2e8f0,
        calendar_base_text: 0x475569,
        selection_ring: 0x1f2937,
    };

    /// 一套外观下的全部颜色。字段必须 `pub` 才能被取色函数读到，
    /// 但 `DARK` / `LIGHT` 两张表是私有的 —— 唯一入口是 [`current`]。
    pub struct Palette {
        pub window_bg: u32,
        pub titlebar_bg: u32,
        pub titlebar_fg: u32,
        pub titlebar_hover: u32,
        pub row_hover: u32,
        pub text: u32,
        pub text_muted: u32,
        pub text_soft: u32,
        pub text_button: u32,
        pub text_disabled: u32,
        pub accent: u32,
        pub accent_hover: u32,
        pub switch_off: u32,
        pub step_bg: u32,
        pub step_bg_hover: u32,
        pub step_bg_disabled: u32,
        pub calendar_base: u32,
        pub calendar_base_text: u32,
        pub selection_ring: u32,
    }

    /// `0` 深色 / `1` 浅色。初值是深色，进程启动后第一个窗口创建时即被校正。
    static APPEARANCE: AtomicU8 = AtomicU8::new(0);

    /// 记录当前系统外观。`WindowAppearance` 的 `Vibrant` 变体只在 macOS 有意义，
    /// 这里与对应的普通变体合并处理。
    pub fn set_appearance(appearance: WindowAppearance) {
        let light = match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => false,
            WindowAppearance::Light | WindowAppearance::VibrantLight => true,
        };
        APPEARANCE.store(light as u8, Ordering::Relaxed);
    }

    /// 当前外观的配色表。所有取色都从这里读，用法 `rgb(palette::current().window_bg)`。
    pub fn current() -> &'static Palette {
        if APPEARANCE.load(Ordering::Relaxed) == 1 {
            &LIGHT
        } else {
            &DARK
        }
    }

    // 与外观无关的颜色：对比色与「危险」色，两套外观下取同一个值。
    /// 纯白：深色格上的数字、开关滑块、「喝了」与关闭键悬停时的文字。
    pub const WHITE: u32 = 0xffffff;
    /// 关闭键悬停底色。深浅两套都用同一个红。
    pub const CLOSE_HOVER: u32 = 0xdc2626;

    /// 热力图 1..=8 级的蓝（由浅到深）。两套外观共用同一段蓝：蓝到足够深时
    /// 在白底与深底上都立得住，只有 0 次那一档需要随外观变。
    const CALENDAR_RAMP: [u32; 8] = [
        0xdbeafe, 0xbfdbfe, 0x93c5fd, 0x60a5fa, 0x3b82f6, 0x2563eb, 0x1d4ed8, 0x1e3a8a,
    ];
    /// 热力图等级总数（含 0 次那一档）。
    pub const CALENDAR_LEVELS: usize = CALENDAR_RAMP.len() + 1;
    /// 浅蓝格上的深色数字，让日期在浅蓝底上仍可读。
    const CALENDAR_TEXT_ON_LIGHT: u32 = 0x0f172a;

    /// 日历格底色：0 次用随外观变的底色，其余共用蓝阶。
    pub fn calendar_bg(level: usize) -> u32 {
        match level {
            0 => current().calendar_base,
            _ => CALENDAR_RAMP[level - 1],
        }
    }

    /// 日历格上的日期数字：0 次用弱化色，浅蓝格用深色，深蓝格用白色。
    /// 「1..=4 用深色」这条分界对两套外观都成立，因为两套共用同一段蓝阶。
    pub fn calendar_text(level: usize) -> u32 {
        match level {
            0 => current().calendar_base_text,
            1..=4 => CALENDAR_TEXT_ON_LIGHT,
            _ => WHITE,
        }
    }

    /// 提醒浮层恒为深色：整屏遮罩压暗桌面，浅色的水杯插图与白字才立得住；
    /// 浅色外观下换成透明会让插图整个糊进白底。所以浮层外壳不随外观变化，
    /// 只有主按钮「喝了」用界面强调色。
    pub mod overlay {
        /// 半透明遮罩（此处是 hsla，不是 rgb）。
        pub fn bg() -> gpui_kit::Hsla {
            gpui_kit::hsla(0., 0., 0., 0.85)
        }
        /// 状态句与「跳过」按钮文字。
        pub const TEXT: u32 = 0xcbd5e1;
        /// 「跳过」按钮描边。
        pub const BORDER: u32 = 0x64748b;
    }
}

/// 让一个窗口跟随系统深浅：外观变化时更新全局配色并重绘。
///
/// 必须**每个窗口**都调一次：gpui 的外观观察者是按窗口注册的（Windows 侧来自
/// `ImmersiveColorSet` 的系统广播，每个顶层窗口各收到一份），而配色是全局的。
/// `Subscription` 是 RAII 守卫，不 `detach()` 会在这个函数返回时就被注销掉。
pub fn follow_system_appearance(window: &mut Window) {
    // 窗口创建时 gpui 会现读一次系统外观，用它校正全局值：进程运行期间可能
    // 一个窗口都没开过（托盘常驻），期间切换深浅就没有任何人更新过配色。
    palette::set_appearance(window.appearance());
    window
        .observe_window_appearance(|window, _| {
            palette::set_appearance(window.appearance());
            window.refresh();
        })
        .detach();
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
    count.div_ceil(3).min(palette::CALENDAR_LEVELS - 1)
}

/// 次数 → 热力图取色，等级由 `calendar_level` 给出。
pub fn calendar_color(count: usize) -> gpui_kit::Rgba {
    rgb(palette::calendar_bg(calendar_level(count)))
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
        .bg(rgb(palette::current().titlebar_bg))
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .px_4()
                .text_color(rgb(palette::current().titlebar_fg))
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
/// - `hover_bg` / `hover_fg`：悬停底色与前景色，必须在参数里给出 ——
///   gpui 的 `InteractiveElement::hover` 对同一元素二次调用会 panic，
///   调用方不能再自己补一个 `.hover(..)`。关闭键要单独给白色前景，
///   否则浅色外观下深色的图标会落在红底上。
/// - `icon_size`：图标字号，取 `WINDOW_ICON_SIZE` 或 `ACTION_ICON_SIZE`。
///
/// 返回具体的 `Stateful<Div>`，调用方可以继续链式追加自己的交互
/// （`.cursor_pointer()` / `.occlude()` / `.on_mouse_down(..)` …）。
pub fn titlebar_button(
    id: &'static str,
    hover_bg: u32,
    hover_fg: u32,
    icon_size: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .font_family(ICON_FONT)
        .w(px(TITLEBAR_BUTTON_WIDTH))
        .h(px(TITLEBAR_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(icon_size))
        .text_color(rgb(palette::current().titlebar_fg))
        .hover(move |s| s.bg(rgb(hover_bg)).text_color(rgb(hover_fg)))
}

/// 窗口控制键（最小化 / 最大化 / 关闭）：共用样式之上附加系统控制区语义。
pub fn window_button(label: &'static str, area: WindowControlArea) -> impl IntoElement {
    let (hover_bg, hover_fg) = if area == WindowControlArea::Close {
        (palette::CLOSE_HOVER, palette::WHITE)
    } else {
        (
            palette::current().titlebar_hover,
            palette::current().titlebar_fg,
        )
    };
    let id = match area {
        WindowControlArea::Min => "window-minimize",
        WindowControlArea::Max => "window-maximize",
        WindowControlArea::Close => "window-close",
        WindowControlArea::Drag => "window-drag",
    };
    titlebar_button(id, hover_bg, hover_fg, WINDOW_ICON_SIZE)
        .occlude()
        .window_control_area(area)
        .child(label)
}
