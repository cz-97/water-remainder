use crate::{
    core::{
        config::{INTERVALS, Store, Theme, settings_snapshot, update_settings},
        paths::reminder_image_file,
        scheduler::{RescheduleType, SchedulerCmd},
    },
    platform,
    ui::{image_format, palette, titlebar, window_button},
};
use gpui_kit::{
    App, Context, MouseButton, Window, WindowBounds, WindowControlArea, WindowKind, WindowOptions,
    div, prelude::*, px, rgb, size,
};
use std::{
    fs,
    sync::{Arc, Mutex, mpsc},
};

/// 设置窗的固定尺寸。窗口不可缩放，所以高度必须自己装得下全部内容：
/// 标题栏 38，上下内边距 40，三个分组标题 28×3，三行设置 74×3，
/// 两处分组间距 12×2，「主题」标题 20，主题预览 50，再加上元素间距，合计约 494。
/// 高度随内容增减 —— 预览卡去掉下面的文字后，这里跟着从 600 降回 540。
const SETTINGS_WIDTH: f32 = 520.;
const SETTINGS_HEIGHT: f32 = 540.;

pub struct SettingsWindow {
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<SchedulerCmd>,
    /// 是否已自定义提醒图片。**缓存**而不是每帧 `path.exists()`：`render` 每次重绘都会
    /// 跑一遍，而 GPUI 重绘相当频繁（悬停、窗口尺寸变化、外观切换……），不该为此
    /// 每帧做一次 `stat` 系统调用。它只在「更换图片 / 恢复默认」两个动作后变化，
    /// 而那两处就是唯一的写入点，所以在这里跟着改即可。
    custom_image: bool,
    /// 最近一次改设置**没存住**的原因。有值时界面会就地显示一行提示 ——
    /// 设置是存在磁盘上的，写不进去必须让用户当场知道，否则重启后「设置自己变回去了」。
    save_error: Option<String>,
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self
            .store
            .lock()
            .map(|s| s.settings.clone())
            .unwrap_or_default();
        // 设置里的间隔已被 `normalize_interval` 收敛到 `INTERVALS` 内，所以这里的
        // 索引必然命中；`unwrap_or(0)` 只是防御性兜底，不会造成「显示第 0 档、
        // 点一下就把用户原值覆盖掉」的情况。
        let interval_index = INTERVALS
            .iter()
            .position(|seconds| *seconds == settings.interval_secs)
            .unwrap_or(0);
        let can_decrease = interval_index > 0;
        let can_increase = interval_index + 1 < INTERVALS.len();

        let store = self.store.clone();
        let scheduler = self.scheduler.clone();
        let autostart = div()
            .id("autostart-setting")
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .py_3()
            .rounded_md()
            .hover(|s| s.bg(rgb(palette::current().row_hover)))
            .cursor_pointer()
            .child(setting_copy("开机启动", "登录 Windows 后自动启动喝水提醒"))
            .child(switch(settings.autostart))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    // 先落盘、成功了再动注册表与界面：写失败时 `update_settings`
                    // 已经把内存值回滚了，这里必须让开关保持原样并给出提示。
                    match update_settings(&store, |settings| {
                        settings.autostart = !settings.autostart;
                        settings.autostart
                    }) {
                        Ok(enabled) => {
                            // 注册表写失败也要报出来并回滚设置：开关显示「已开启」
                            // 而 Run 键没写进去，下次开机不会启动，用户无从察觉。
                            match platform::set_autostart(enabled) {
                                Ok(()) => this.save_error = None,
                                Err(error) => {
                                    let _ = update_settings(&store, |settings| {
                                        settings.autostart = !enabled
                                    });
                                    platform::set_autostart(!enabled).ok();
                                    this.save_error = Some(error);
                                }
                            }
                        }
                        Err(error) => this.save_error = Some(error.to_string()),
                    }
                    cx.notify();
                }),
            );

        let store = self.store.clone();
        let scheduler_for_decrease = scheduler.clone();
        let decrease = step_button("−", can_decrease).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                if can_decrease {
                    this.save_error =
                        set_interval(&store, &scheduler_for_decrease, interval_index - 1);
                    cx.notify();
                }
            }),
        );
        let store = self.store.clone();
        let scheduler_for_increase = scheduler.clone();
        let increase = step_button("+", can_increase).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                if can_increase {
                    this.save_error =
                        set_interval(&store, &scheduler_for_increase, interval_index + 1);
                    cx.notify();
                }
            }),
        );
        let interval = div()
            .flex()
            .items_center()
            .gap_2()
            .child(decrease)
            .child(
                div()
                    .w(px(86.))
                    .text_center()
                    .text_color(rgb(palette::current().text))
                    .child(format!("{} 分钟", settings.interval_secs / 60)),
            )
            .child(increase);
        let interval_row = div()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .py_3()
            .rounded_md()
            .hover(|s| s.bg(rgb(palette::current().row_hover)))
            .child(setting_copy("提醒间隔", "两次提醒之间的等待时间"))
            .child(interval);

        let custom = self.custom_image;
        let change = text_button("change-image", "更换", true).on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                if let Some(path) = platform::pick_image_file()
                    && let Ok(bytes) = fs::read(&path)
                    && image_format(&bytes).is_some()
                {
                    let _ = fs::write(reminder_image_file(), &bytes);
                    // 写完重新判定一次真实状态：写失败时不能显示成「已自定义」，
                    // 否则用户会以为换图成功了。这里是一次点击一次，不做每帧 stat。
                    this.custom_image = reminder_image_file().exists();
                }
                cx.notify();
            }),
        );
        let mut reset = text_button("reset-image", "恢复默认", custom);
        if custom {
            reset = reset.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    let _ = fs::remove_file(reminder_image_file());
                    this.custom_image = reminder_image_file().exists();
                    cx.notify();
                }),
            );
        }
        let image_row = div()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .py_3()
            .rounded_md()
            .child(setting_copy(
                "提醒图片",
                "提醒浮层中央的插图，可替换为本地图片",
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(change)
                    .child(reset),
            );

        let title_bar = titlebar("设置", window_button("\u{e8bb}", WindowControlArea::Close));

        // 落盘失败的提示条：固定在正文最上方，用户改完立刻能看到。
        // 设置是持久化到磁盘的，写不进去却显示成功，重启后设置会「自己变回去」。
        let save_error = self.save_error.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(palette::current().window_bg))
            .text_color(rgb(palette::current().text))
            .child(title_bar)
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_5()
                    // 窗口不可缩放，内容必须自己装得下：`overflow_y_scroll`
                    // 避免以后再加一行设置时把底部条目裁掉（且没有滚动条可滚）。
                    .overflow_y_scroll()
                    .when_some(save_error, |this, message| {
                        this.child(
                            div()
                                .mb_2()
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .text_sm()
                                .text_color(rgb(palette::overlay::WARNING))
                                .child(format!("设置没能保存：{message}")),
                        )
                    })
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(palette::current().text_muted))
                            .mb_2()
                            .child("提醒"),
                    )
                    .child(interval_row)
                    .child(image_row)
                    .child(div().h(px(12.)))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(palette::current().text_muted))
                            .mb_2()
                            .child("启动"),
                    )
                    .child(autostart)
                    .child(div().h(px(12.)))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(palette::current().text_muted))
                            .mb_2()
                            .child("外观"),
                    )
                    // 主题：三张预览卡片（跟随系统 / 浅色 / 深色），选中项用强调色描边。
                    // 系统跟随的预览用当前实际生效的那一套色（跟随时就是系统的深浅，
                    // 强制时就是它自己），所以卡片内容始终是「选了它之后界面长什么样」。
                    .child(div().text_color(rgb(palette::current().text)).child("主题"))
                    .child(theme_row(self.store.clone(), cx)),
            )
    }
}

/// 主题选择区：浅色 / 深色两张预览在左，「跟随系统」开关在**最右**。
///
/// 「跟随系统」是**开关**而不是第三张预览 —— 它的含义是「别管我，听系统的」，
/// 画成一张具体配色反而会误导（它并不是第三种外观）。开关打开时两张预览
/// **都不选中**（此刻由系统决定用哪张）；关掉并手动点了某一张，那张才描边点亮。
fn theme_row(store: Arc<Mutex<Store>>, cx: &mut Context<SettingsWindow>) -> impl IntoElement {
    let theme = settings_snapshot(&store).theme;
    // `.px_4` 让这一行与「提醒」「启动」里的行左对齐（那些行各自带 px_4 卡片内边距），
    // 否则主题行会贴着窗口边缘、看起来比别的组矮一截、不协调。
    let mut row = div().flex().items_center().gap_2().px_4().py_2();
    // 折叠成 `impl IntoElement` 需要一个确定类型，所以先建好再依次追加。
    for choice in Theme::CHOICES {
        row = row.child(theme_card(choice, theme, &store, cx));
    }
    // 弹性占位把开关推到最右，和「间隔」行的右对齐控件形成呼应。
    row = row
        .child(div().flex_1())
        .child(follow_system_switch(theme, &store, cx));
    row
}

/// 「跟随系统」开关。开着等价于 Theme::System。
fn follow_system_switch(
    current: Theme,
    store: &Arc<Mutex<Store>>,
    cx: &mut Context<SettingsWindow>,
) -> impl IntoElement {
    let store = store.clone();
    div()
        .flex()
        .items_center()
        .gap_2()
        .cursor_pointer()
        // 开关上只写「跟随系统」四个字：右边两张预览已经自带深浅信息，
        // 开关的职责只是「听不听系统的」，不需要再画一张预览图。
        .child(
            div()
                .text_color(rgb(palette::current().text))
                .child(Theme::System.label()),
        )
        .child(switch(current == Theme::System))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                // 关掉时落到「系统当前实际是深还是浅」那一张，界面上立刻有实感。
                apply_theme(
                    toggled_follow(current, palette::is_light()),
                    &store,
                    this,
                    cx,
                );
            }),
        )
}

/// 单张预览卡。只有真正选中（手动选中且与当前一致）时才描边点亮。
/// 卡片**只有预览、没有文字** —— 深浅本身画得清清楚楚，再配一行字是冗余。
fn theme_card(
    theme: Theme,
    current: Theme,
    store: &Arc<Mutex<Store>>,
    cx: &mut Context<SettingsWindow>,
) -> impl IntoElement {
    let selected = preview_is_selected(current, theme);
    let store = store.clone();
    let preview = palette::for_preview(theme == Theme::Light);

    div()
        // 固定宽度而不是 `flex_1`：这一行右边要留给「跟随系统」开关，预览铺满会把
        // 开关挤到换行。150px 足够画清缩略图里的标题栏和内容条。
        .w(px(150.))
        .h(px(46.))
        .flex_shrink_0()
        .rounded_md()
        .overflow_hidden()
        .border_2()
        .border_color(rgb(if selected {
            palette::current().accent
        } else {
            palette::current().step_bg
        }))
        .cursor_pointer()
        // 悬浮时把未选中的描边提亮一点，给出「可以点」的暗示。
        .when(!selected, |this| {
            this.hover(|s| s.border_color(rgb(palette::current().row_hover)))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                apply_theme(theme, &store, this, cx);
            }),
        )
        .child(theme_swatch(preview))
}

/// 迷你窗口预览：一条标题栏 + 两条内容横杠 + 一个小色块，用对应主题的配色画。
/// 四角圆角由外层卡片的圆角加裁剪裁出来。
fn theme_swatch(preview: &palette::Palette) -> impl IntoElement {
    div()
        .h(px(46.))
        .w_full()
        .flex()
        .flex_col()
        .bg(rgb(preview.window_bg))
        // 标题栏
        .child(div().h(px(10.)).w_full().bg(rgb(preview.titlebar_bg)))
        // 内容：一个小方块 + 两条横杠，暗示主界面布局
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .child(
                    div()
                        .w(px(10.))
                        .h(px(10.))
                        .rounded_sm()
                        .bg(rgb(preview.accent)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .flex_1()
                        .child(
                            div()
                                .h(px(3.))
                                .w(px(38.))
                                .rounded_full()
                                .bg(rgb(preview.text)),
                        )
                        .child(
                            div()
                                .h(px(3.))
                                .w(px(26.))
                                .rounded_full()
                                .bg(rgb(preview.text_muted)),
                        ),
                ),
        )
}

/// 改主题：先落盘，成功后写全局配色并让所有窗口重绘；失败交给 save_error 提示条。
fn apply_theme(
    theme: Theme,
    store: &Arc<Mutex<Store>>,
    view: &mut SettingsWindow,
    cx: &mut Context<SettingsWindow>,
) {
    match update_settings(store, |settings| settings.theme = theme) {
        Ok(()) => {
            palette::set_preference(theme);
            cx.refresh_windows();
            view.save_error = None;
        }
        Err(error) => view.save_error = Some(error.to_string()),
    }
    cx.notify();
}

/// 点「跟随系统」开关之后会变成哪个主题。
///
/// 开关开着 ⇔ 跟随系统，所以关掉它就得落到一个**具体**的深/浅上；选哪个取
/// 「系统此刻实际是深是浅」，而不是「上次手动选的那个」—— 用户点这个开关的意图
/// 是「别跟随」，此刻眼睛看到的正是系统实际外观。抽成纯函数是为了能测。
fn toggled_follow(current: Theme, system_is_light: bool) -> Theme {
    match current {
        Theme::System => {
            if system_is_light {
                Theme::Light
            } else {
                Theme::Dark
            }
        }
        // 已经手动选过了，再点一次就是恢复跟随。
        _ => Theme::System,
    }
}

/// 单张预览是否应当描边点亮。
///
/// 跟随系统时**两张都不选中**：此刻由系统决定用哪张，圈住其中任何一张都是在撒谎
/// （界面说「你选的是深色」，实际可能是浅色）。跟着系统的语义只由开关表示。
fn preview_is_selected(current: Theme, card: Theme) -> bool {
    current == card
}

fn setting_copy(title: &'static str, description: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_color(rgb(palette::current().text)).child(title))
        .child(
            div()
                .text_sm()
                .text_color(rgb(palette::current().text_muted))
                .child(description),
        )
}

fn switch(enabled: bool) -> impl IntoElement {
    div()
        .w(px(36.))
        .h(px(20.))
        .rounded_full()
        .p(px(2.))
        .flex()
        .items_center()
        .justify_start()
        .bg(if enabled {
            rgb(palette::current().accent)
        } else {
            rgb(palette::current().switch_off)
        })
        .child(
            div()
                .size(px(16.))
                .rounded_full()
                .bg(rgb(palette::WHITE))
                .when(enabled, |this| this.ml(px(16.))),
        )
}

fn text_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let mut button = div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_sm()
        .text_sm()
        .text_color(if enabled {
            rgb(palette::current().text_button)
        } else {
            rgb(palette::current().text_disabled)
        })
        .bg(if enabled {
            rgb(palette::current().step_bg)
        } else {
            rgb(palette::current().step_bg_disabled)
        })
        .child(label);
    if enabled {
        button = button
            .cursor_pointer()
            .hover(|s| s.bg(rgb(palette::current().step_bg_hover)));
    }
    button
}

fn step_button(label: &'static str, enabled: bool) -> gpui_kit::Div {
    div()
        .w(px(28.))
        .h(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .text_lg()
        .text_color(if enabled {
            rgb(palette::current().text)
        } else {
            rgb(palette::current().text_disabled)
        })
        .bg(if enabled {
            rgb(palette::current().step_bg)
        } else {
            rgb(palette::current().step_bg_disabled)
        })
        .when(enabled, |this| {
            this.hover(|s| s.bg(rgb(palette::current().step_bg_hover)))
                .cursor_pointer()
        })
        .child(label)
}

/// 切到 `INTERVALS[index]`。返回 `None` 表示成功，`Some(原因)` 表示没存住。
///
/// 只有在**真的落盘之后**才通知调度器改间隔：文件没写成功却先改了调度节奏，
/// 会让本次运行的时间间隔与下次启动读到的不一致。
fn set_interval(
    store: &Arc<Mutex<Store>>,
    scheduler: &mpsc::Sender<SchedulerCmd>,
    index: usize,
) -> Option<String> {
    let &seconds = INTERVALS.get(index)?;
    match update_settings(store, |settings| settings.interval_secs = seconds) {
        Ok(()) => {
            let _ = scheduler.send(SchedulerCmd::Reschedule(RescheduleType::ChangeInterval(
                seconds,
            )));
            None
        }
        Err(error) => Some(error.to_string()),
    }
}

pub fn open_settings_window(
    cx: &mut App,
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<SchedulerCmd>,
) {
    if let Some(handle) = cx
        .windows()
        .iter()
        .find_map(|w| w.downcast::<SettingsWindow>())
    {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let _ = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::centered(
                size(px(SETTINGS_WIDTH), px(SETTINGS_HEIGHT)),
                cx,
            )),
            titlebar: None,
            kind: WindowKind::Normal,
            is_resizable: false,
            ..Default::default()
        },
        move |window, cx| {
            crate::ui::follow_system_appearance(window);
            // 只在开窗时判一次文件是否存在，之后由两个按钮维护这份缓存。
            let custom_image = reminder_image_file().exists();
            cx.new(|_| SettingsWindow {
                store,
                scheduler,
                custom_image,
                save_error: None,
            })
        },
    );
}

pub fn close_settings_window(cx: &mut App) {
    if let Some(handle) = cx
        .windows()
        .iter()
        .find_map(|w| w.downcast::<SettingsWindow>())
    {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 点「跟随系统」开关之后会变成哪个主题。
    ///
    /// 开关开着 ⇔ 跟随系统，所以关掉它就得落到一个**具体**的深/浅上；选哪个
    /// 取「系统此刻实际是深是浅」，而不是「上次手动选的那个」—— 用户点这个开关
    /// 的意图是「别跟随」，此刻眼睛看到的正是系统实际外观。抽成纯函数是为了能测。
    #[test]
    fn toggling_follow_lands_on_the_system_current_look() {
        assert_eq!(toggled_follow(Theme::System, true), Theme::Light);
        assert_eq!(toggled_follow(Theme::System, false), Theme::Dark);
        // 已经手动选过了，再点一次就是恢复跟随。
        assert_eq!(toggled_follow(Theme::Light, false), Theme::System);
        assert_eq!(toggled_follow(Theme::Dark, true), Theme::System);
    }

    /// 跟随系统时**两张预览都不选中**：此刻由系统决定用哪张，圈住其中任何一张
    /// 都是在撒谎。跟着系统的语义只由左边那个开关表示。
    #[test]
    fn no_preview_is_highlighted_while_following_the_system() {
        for system_is_light in [true, false] {
            assert!(!preview_is_selected(Theme::System, Theme::Light));
            assert!(!preview_is_selected(Theme::System, Theme::Dark));
            let _ = system_is_light;
        }
    }

    /// 手动选中时，只有对应那张点亮。
    #[test]
    fn the_manually_chosen_preview_is_highlighted() {
        assert!(preview_is_selected(Theme::Light, Theme::Light));
        assert!(!preview_is_selected(Theme::Light, Theme::Dark));
        assert!(preview_is_selected(Theme::Dark, Theme::Dark));
        assert!(!preview_is_selected(Theme::Dark, Theme::Light));
    }

    /// 设置窗不可缩放，高度必须自己装得下全部内容；主题那一行是后来加的，
    /// 固定高度如果忘了跟着调，底部就会被裁掉。
    ///
    /// 断言的是「两个常量之间的关系」，编译器能在编译期判定其真伪，因此
    /// clippy 会报 `assertions_on_constants`。这里**刻意保留**：它是一份可执行的
    /// 布局记账 —— 以后有人加了设置项却忘了调 `SETTINGS_HEIGHT`，测试就会失败。
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn settings_window_is_tall_enough_for_its_content() {
        // 按当前各元素真实高度求和：标题栏 + 上下内边距 + 三个分组标题
        // + 三行设置 + 两处分组间距 + 「主题」标题 + 主题预览(46 + 2px 描边)
        // + 元素间距。
        let content = 38. + 2. * 20. + 3. * 28. + 3. * 74. + 2. * 12. + 20. + 50.;
        assert!(
            SETTINGS_HEIGHT >= content,
            "窗口高 {} 装不下约 {} 的内容，主题预览会被裁掉",
            SETTINGS_HEIGHT,
            content
        );
        // 左边两张预览各 150，加上「跟随系统」开关与其文字约 130，两处间隙 16，
        // 再加左右内边距 32：合计约 480，窗口 520 刚好放得下且不换行。
        assert!(
            2. * 150. + 130. + 2. * 8. + 2. * 16. <= SETTINGS_WIDTH,
            "窗口宽 {} 放不下两张预览加「跟随系统」开关",
            SETTINGS_WIDTH
        );
    }
}
