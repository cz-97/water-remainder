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

/// 三张主题预览卡片。返回一整行，卡片本身负责响应点击。
fn theme_row(store: Arc<Mutex<Store>>, cx: &mut Context<SettingsWindow>) -> impl IntoElement {
    let row = div().flex().gap_2();
    // 折叠成 `impl IntoElement` 需要一个确定的类型，所以先把卡片收集起来再渲染。
    let mut row = row;
    for theme in Theme::ALL {
        row = row.child(theme_card(theme, &store, cx));
    }
    row
}

/// 单张主题卡片：上方一个迷你窗口预览，下方是名称 + 选中角标。
fn theme_card(
    theme: Theme,
    store: &Arc<Mutex<Store>>,
    cx: &mut Context<SettingsWindow>,
) -> impl IntoElement {
    let selected = settings_snapshot(store).theme == theme;
    let store = store.clone();
    // 预览用哪套色：选中「跟随系统」时按当前实际外观画，另外两个直接用对应那套。
    let preview_light = match theme {
        Theme::System => palette::is_light(),
        Theme::Light => true,
        Theme::Dark => false,
    };
    let preview = palette::for_preview(preview_light);

    let badge = div()
        .w(px(9.))
        .h(px(9.))
        .rounded_full()
        .bg(rgb(palette::WHITE));

    let card = div()
        .flex()
        .flex_1()
        .flex_col()
        .gap_2()
        .p_2()
        .rounded_lg()
        .overflow_hidden()
        .border_2()
        .border_color(rgb(if selected {
            palette::current().accent
        } else {
            palette::current().step_bg
        }))
        .bg(rgb(palette::current().step_bg))
        .cursor_pointer()
        // 悬浮时把未选中的描边提亮一点，给出「可以点」的暗示。
        .when(!selected, |this| {
            this.hover(|s| s.border_color(rgb(palette::current().row_hover)))
        })
        .child(theme_swatch(preview))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(div().text_sm().child(theme.label()))
                // 选中角标紧挨文字，占位固定，不让选中/未选中导致文字左右跳动。
                .child(
                    div()
                        .w(px(9.))
                        .h(px(9.))
                        .when(selected, |this| this.child(badge)),
                ),
        );

    card.on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, _, _, cx| {
            match update_settings(&store, |settings| settings.theme = theme) {
                Ok(()) => {
                    // 立即生效：把新主题写进全局并让所有窗口重绘，主窗/浮层立刻跟着变。
                    palette::set_preference(theme);
                    cx.refresh_windows();
                    this.save_error = None;
                }
                Err(error) => this.save_error = Some(error.to_string()),
            }
            cx.notify();
        }),
    )
}

/// 迷你窗口预览：一条标题栏 + 两条内容横杠 + 一个小色块，用对应主题的配色画。
fn theme_swatch(preview: &palette::Palette) -> impl IntoElement {
    div()
        .h(px(46.))
        .w_full()
        .rounded_md()
        .overflow_hidden()
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
            window_bounds: Some(WindowBounds::centered(size(px(520.), px(520.)), cx)),
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
