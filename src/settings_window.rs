use crate::{
    config::{INTERVALS, Store, save_store},
    platform,
    scheduler::AppCmd,
    ui::window_button,
};
use gpui::{
    App, Context, MouseButton, Window, WindowBackgroundAppearance, WindowBounds, WindowControlArea,
    WindowKind, WindowOptions, div, prelude::*, px, rgb, size,
};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub struct SettingsWindow {
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<AppCmd>,
}

impl Render for SettingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self
            .store
            .lock()
            .map(|s| s.settings.clone())
            .unwrap_or_else(|_| crate::config::Settings {
                interval_secs: crate::config::DEFAULT_INTERVAL,
                autostart: false,
                startup_remind: true,
            });
        let interval_index = INTERVALS
            .iter()
            .position(|(seconds, _)| *seconds == settings.interval_secs)
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
            .hover(|s| s.bg(rgb(0x292929)))
            .cursor_pointer()
            .child(setting_copy("开机启动", "登录 Windows 后自动启动喝水提醒"))
            .child(switch(settings.autostart))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, _, cx| {
                    if let Ok(mut store) = store.lock() {
                        store.settings.autostart = !store.settings.autostart;
                        platform::set_autostart(store.settings.autostart);
                        save_store(&store);
                    }
                    cx.notify();
                }),
            );

        let store = self.store.clone();
        let startup_remind = div()
            .id("startup-remind-setting")
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .py_3()
            .rounded_md()
            .hover(|s| s.bg(rgb(0x292929)))
            .cursor_pointer()
            .child(setting_copy("启动时立即提醒", "应用启动后立刻显示一次提醒"))
            .child(switch(settings.startup_remind))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, _, cx| {
                    if let Ok(mut store) = store.lock() {
                        store.settings.startup_remind = !store.settings.startup_remind;
                        save_store(&store);
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
                    .text_color(rgb(0xe5e7eb))
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
            .hover(|s| s.bg(rgb(0x292929)))
            .child(setting_copy("提醒间隔", "两次提醒之间的等待时间"))
            .child(interval);

        let titlebar = div()
            .h(px(38.))
            .flex()
            .items_center()
            .bg(rgb(0x1d1d1d))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .px_4()
                    .text_color(rgb(0xe0f2fe))
                    .font_weight(gpui::FontWeight::BOLD)
                    .window_control_area(WindowControlArea::Drag)
                    .child("设置"),
            )
            .child(window_button("\u{e8bb}", WindowControlArea::Close));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1f1f1f))
            .text_color(rgb(0xe5e7eb))
            .child(titlebar)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_5()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x94a3b8))
                            .mb_2()
                            .child("提醒"),
                    )
                    .child(interval_row)
                    .child(div().h(px(12.)))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x94a3b8))
                            .mb_2()
                            .child("启动"),
                    )
                    .child(autostart)
                    .child(startup_remind),
            )
    }
}

fn setting_copy(title: &'static str, description: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_color(rgb(0xe5e7eb)).child(title))
        .child(div().text_sm().text_color(rgb(0x94a3b8)).child(description))
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
            rgb(0x60a5fa)
        } else {
            rgb(0x475569)
        })
        .child(
            div()
                .size(px(16.))
                .rounded_full()
                .bg(rgb(0xffffff))
                .when(enabled, |this| this.ml(px(16.))),
        )
}

fn step_button(label: &'static str, enabled: bool) -> gpui::Div {
    div()
        .w(px(28.))
        .h(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .text_lg()
        .text_color(if enabled {
            rgb(0xe5e7eb)
        } else {
            rgb(0x64748b)
        })
        .bg(if enabled {
            rgb(0x334155)
        } else {
            rgb(0x252b33)
        })
        .when(enabled, |this| {
            this.hover(|s| s.bg(rgb(0x475569))).cursor_pointer()
        })
        .child(label)
}

fn set_interval(store: &Arc<Mutex<Store>>, scheduler: &mpsc::Sender<AppCmd>, index: usize) {
    let Some(&(seconds, _)) = INTERVALS.get(index) else {
        return;
    };
    if let Ok(mut store) = store.lock() {
        store.settings.interval_secs = seconds;
        save_store(&store);
        let _ = scheduler.send(AppCmd::Reset(Duration::from_secs(seconds)));
    }
}

pub fn open_settings_window(
    cx: &mut App,
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<AppCmd>,
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
            window_bounds: Some(WindowBounds::centered(size(px(520.), px(420.)), cx)),
            titlebar: None,
            kind: WindowKind::Normal,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Opaque,
            ..Default::default()
        },
        move |_, cx| cx.new(|_| SettingsWindow { store, scheduler }),
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
