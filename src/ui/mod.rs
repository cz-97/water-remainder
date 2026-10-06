pub mod main_window;
pub mod reminder_window;
pub mod settings_window;

use gpui_kit::{
    Div, FontWeight, ImageFormat, Stateful, Window, WindowControlArea, div, prelude::*, px, rgb,
};

/// 界面配色唯一来源。深浅两套切换由**用户设置**决定（跟随系统 / 强制浅色 / 强制深色）：
/// [`palette::set_preference`] 记录用户选择（设置窗写盘时一并写入），
/// [`palette::set_system_appearance`] 由 [`follow_system_appearance`] 在窗口创建时写入、
/// 之后每次系统切换深浅再写一次。只有跟随系统时两者才会共同决定外观 ——
/// 用户强制了浅/深，系统切深浅就不该再改动界面。
///
/// 全局只存一个外观值就够：颜色只在 GPUI 主线程上被读（每次 `render`）与写
/// （外观观察者回调），`AtomicU8` 既无锁也不会撕裂。
pub mod palette {
    use crate::core::config::Theme;
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

    /// 用户选的主题：`0` 跟随系统 / `1` 浅色 / `2` 深色。初值跟随系统。
    static PREFERENCE: AtomicU8 = AtomicU8::new(0);

    /// 系统当前外观：`0` 深色 / `1` 浅色。**只在跟随系统时才决定最终外观**；
    /// 进程启动后第一个窗口创建时即被校正。
    static SYSTEM_LIGHT: AtomicU8 = AtomicU8::new(0);

    /// 记录系统外观。`WindowAppearance` 的 `Vibrant` 变体只在 macOS 有意义，
    /// 这里与对应的普通变体合并处理。
    pub fn set_system_appearance(appearance: WindowAppearance) {
        let light = match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => false,
            WindowAppearance::Light | WindowAppearance::VibrantLight => true,
        };
        SYSTEM_LIGHT.store(light as u8, Ordering::Relaxed);
    }

    /// 记录用户选的主题。
    pub fn set_preference(theme: Theme) {
        PREFERENCE.store(
            match theme {
                Theme::System => 0,
                Theme::Light => 1,
                Theme::Dark => 2,
            },
            Ordering::Relaxed,
        );
    }

    /// 当前生效的主题（用户选择）。
    pub fn preference() -> Theme {
        match PREFERENCE.load(Ordering::Relaxed) {
            1 => Theme::Light,
            2 => Theme::Dark,
            _ => Theme::System,
        }
    }

    /// 此刻界面实际该用哪张配色表：用户指定了浅/深就听用户的，否则听系统的。
    pub fn is_light() -> bool {
        match preference() {
            Theme::Light => true,
            Theme::Dark => false,
            Theme::System => SYSTEM_LIGHT.load(Ordering::Relaxed) == 1,
        }
    }

    /// 当前外观的配色表。所有取色都从这里读，用法 `rgb(palette::current().window_bg)`。
    pub fn current() -> &'static Palette {
        if is_light() { &LIGHT } else { &DARK }
    }

    /// 按「是否浅色」直接取表，不看用户当前选了什么。
    /// 只给设置窗的**主题预览卡片**用 —— 它要在界面上同时画出三张不同深浅的
    /// 迷你窗口，而此时生效的只有其中一张。
    pub fn for_preview(light: bool) -> &'static Palette {
        if light { &LIGHT } else { &DARK }
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
        /// 记录写入失败时的提示文字。浮层恒为深色底，所以取一个暗底上够醒目的浅红，
        /// 两套外观下都是同一个值。
        pub const WARNING: u32 = 0xfca5a5;
    }
}

/// 让一个窗口跟随系统深浅：系统外观变化时更新全局配色并重绘。
///
/// **必须每个窗口都调一次**：gpui 的外观观察者是按窗口注册的（Windows 侧来自
/// `ImmersiveColorSet` 的系统广播，每个顶层窗口各收到一份），而配色是全局的。
/// `Subscription` 是 RAII 守卫，不 `detach()` 会在这个函数返回时就被注销掉。
///
/// 用户强制了浅色/深色时，系统切深浅不会改变外观 —— `palette::is_light` 会以
/// 用户选择为准，这里照常记录系统外观只是为了让「切回跟随系统」时立刻拿到当前值。
pub fn follow_system_appearance(window: &mut Window) {
    // 窗口创建时 gpui 会现读一次系统外观：进程运行期间可能一个窗口都没开过
    // （托盘常驻），期间切换深浅就没有任何人更新过这个值。
    palette::set_system_appearance(window.appearance());
    window
        .observe_window_appearance(|window, _| {
            palette::set_system_appearance(window.appearance());
            window.refresh();
        })
        .detach();
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
pub const TITLEBAR_BUTTON_WIDTH: f32 = 46.;
/// 标题栏图标字体。`Segoe Fluent Icons` 是 Win11 的图标字体，Win10 只有
/// `Segoe MDL2 Assets`；两者覆盖本项目用到的码位（设置 `E713`、最小化 `E921`、
/// 最大化 `E922`、还原 `E923`、关闭 `E8BB` —— 已逐个比对过两张字体表）。
/// 所以把 MDL2 作为回退字体，缺少 Fluent 的机器上就不会渲染成方块。
const ICON_FONT: &str = "Segoe Fluent Icons";
const ICON_FONT_FALLBACK: &str = "Segoe MDL2 Assets";
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

/// 三段式标题栏：`left` / `right` 是两侧的按钮组，标题落在窗口正中。
///
/// 三段都取 `flex_1`（`flex-basis: 0`），于是各分到栏宽的三分之一，标题那一段
/// 的中心正好是窗口中心。按内容宽度排列是不行的：主窗左侧两个功能键（92px）、
/// 右侧三个窗口控制键（138px），标题会向左偏 23px。
///
/// 两侧的容器本身**不能**挂 `WindowControlArea::Drag`。命中测试遍历
/// `window_control_hitboxes` 取第一个命中的，而 hitbox 是 paint 顺序入队的，
/// 父容器的 Drag 会排在子按钮之前 —— 一挂上去，最小化/最大化/关闭就全被抢走。
/// 空白处的拖拽能力改由与按钮**不重叠**的弹性占位块提供（`drag_filler`）：
/// 它排在按钮之后 siblings，命中测试走到它时说明指针不在任何按钮上。
pub fn titlebar_split(
    left: impl IntoElement,
    title: &'static str,
    right: impl IntoElement,
) -> impl IntoElement {
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
                .justify_start()
                .child(left)
                .child(drag_filler()),
        )
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(palette::current().titlebar_fg))
                .font_weight(FontWeight::BOLD)
                .window_control_area(WindowControlArea::Drag)
                .child(title),
        )
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_end()
                .child(drag_filler())
                .child(right),
        )
}

/// 标题栏里只负责「可拖拽」的空块：吃掉一侧的剩余宽度，让按钮仍贴边。
fn drag_filler() -> impl IntoElement {
    div()
        .flex_1()
        .h_full()
        .window_control_area(WindowControlArea::Drag)
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
        .font(gpui_kit::Font {
            family: ICON_FONT.into(),
            // 回退链：Win10 没有 Fluent 图标字体，缺了它这几个码位会画成方块。
            fallbacks: Some(gpui_kit::FontFallbacks::from_fonts(vec![
                ICON_FONT_FALLBACK.to_string(),
            ])),
            ..Default::default()
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 分档决定日历格底色，而色阶索引**不能越界**：`calendar_bg` 只覆盖
    /// `0..CALENDAR_LEVELS`，越界会在 `CALENDAR_RAMP[level - 1]` 上 panic。
    #[test]
    fn calendar_level_stays_within_the_ramp() {
        for count in [0, 1, 2, 3, 7, 8, 9, 100, 1_000, usize::MAX] {
            let level = calendar_level(count);
            assert!(
                level < palette::CALENDAR_LEVELS,
                "count={count} 得到 level={level}，超出色阶范围"
            );
        }
    }

    #[test]
    fn calendar_level_buckets_every_three_drinks() {
        assert_eq!(calendar_level(0), 0, "0 次单独一档");
        assert_eq!(calendar_level(1), 1);
        assert_eq!(calendar_level(2), 1);
        assert_eq!(calendar_level(3), 1, "3 次仍是第 1 档（div_ceil）");
        assert_eq!(calendar_level(4), 2);
        assert_eq!(calendar_level(6), 2);
        assert_eq!(calendar_level(7), 3);
    }

    /// 封顶后不再增长，否则深色档会越界。
    #[test]
    fn calendar_level_saturates_at_the_top_level() {
        let top = palette::CALENDAR_LEVELS - 1;
        assert_eq!(calendar_level(top * 3), top);
        assert_eq!(calendar_level(usize::MAX), top);
    }

    /// 取色必须能对每一档算出颜色而不 panic —— 这是上面那条不变式的实际后果。
    #[test]
    fn calendar_color_is_defined_for_every_count() {
        for count in 0..=(palette::CALENDAR_LEVELS * 3 + 5) {
            let _ = calendar_color(count);
        }
    }

    #[test]
    fn image_format_reads_magic_numbers_not_extensions() {
        assert_eq!(
            image_format(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0]),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            image_format(&[0xff, 0xd8, 0xff, 0xe0]),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(image_format(b"GIF89a....."), Some(ImageFormat::Gif));
        assert_eq!(image_format(b"GIF87a....."), Some(ImageFormat::Gif));
        assert_eq!(image_format(b"BM......"), Some(ImageFormat::Bmp));
        assert_eq!(
            image_format(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(ImageFormat::Webp)
        );
        assert_eq!(
            image_format(&[0x49, 0x49, 0x2a, 0x00]),
            Some(ImageFormat::Tiff)
        );
        assert_eq!(
            image_format(&[0x4d, 0x4d, 0x00, 0x2a]),
            Some(ImageFormat::Tiff)
        );
        assert_eq!(
            image_format(&[0x00, 0x00, 0x01, 0x00]),
            Some(ImageFormat::Ico)
        );
    }

    #[test]
    fn image_format_rejects_unknown_and_truncated_input() {
        assert_eq!(image_format(b""), None);
        assert_eq!(image_format(b"not an image"), None);
        assert_eq!(image_format(&[0x89, b'P']), None, "签名不全时不能误判");
        // RIFF 但不是 WEBP：只匹配前 4 字节会把这个当成 WebP。
        assert_eq!(image_format(b"RIFF\0\0\0\0WAVEfmt "), None);
    }
}
