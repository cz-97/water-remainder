use crate::{
    core::{
        config::{INTERVALS, Store, update_settings},
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
                cx.listener(move |_, _, _, cx| {
                    let enabled = update_settings(&store, |settings| {
                        settings.autostart = !settings.autostart;
                        settings.autostart
                    });
                    if let Some(enabled) = enabled {
                        platform::set_autostart(enabled);
                    }
                    cx.notify();
                }),
            );

        let store = self.store.clone();
        let scheduler_for_decrease = scheduler.clone();
        let decrease = step_button("−", can_decrease).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, _, _, cx| {
                if can_decrease {
                    set_interval(&store, &scheduler_for_decrease, interval_index - 1);
                    cx.notify();
                }
            }),
        );
        let store = self.store.clone();
        let scheduler_for_increase = scheduler.clone();
        let increase = step_button("+", can_increase).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, _, _, cx| {
                if can_increase {
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

        let custom = reminder_image_file().exists();
        let change = text_button("change-image", "更换", true).on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _, _, cx| {
                if let Some(path) = platform::pick_image_file() {
                    if let Ok(bytes) = fs::read(&path) {
                        if image_format(&bytes).is_some() {
                            let _ = fs::write(reminder_image_file(), &bytes);
                        }
                    }
                }
                cx.notify();
            }),
        );
        let mut reset = text_button("reset-image", "恢复默认", custom);
        if custom {
            reset = reset.on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| {
                    let _ = fs::remove_file(reminder_image_file());
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
                    // 主题直接跟随系统深浅，没有开关：这里只告诉用户当前生效的是哪一种，
                    // 以及去哪里改。否则“界面没变”看起来像 bug。
                    .child(setting_copy("主题", "跟随系统的应用颜色设置，无需手动切换")),
            )
    }
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

fn set_interval(store: &Arc<Mutex<Store>>, scheduler: &mpsc::Sender<SchedulerCmd>, index: usize) {
    let Some(&seconds) = INTERVALS.get(index) else {
        return;
    };
    if update_settings(store, |settings| settings.interval_secs = seconds).is_some() {
        let _ = scheduler.send(SchedulerCmd::Reschedule(RescheduleType::ChangeInterval(
            seconds,
        )));
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
            cx.new(|_| SettingsWindow { store, scheduler })
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
