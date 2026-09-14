use crate::{
    config::{INTERVALS, Store, update_settings},
    platform,
    scheduler::{RescheduleType, SchedulerCmd},
    ui::{palette, titlebar, window_button},
};
use gpui::{
    App, Context, MouseButton, Window, WindowBounds, WindowControlArea, WindowKind, WindowOptions,
    div, prelude::*, px, rgb, size,
};
use std::sync::{Arc, Mutex, mpsc};

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
            .hover(|s| s.bg(rgb(palette::ROW_HOVER)))
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
                    .text_color(rgb(palette::TEXT))
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
            .hover(|s| s.bg(rgb(palette::ROW_HOVER)))
            .child(setting_copy("提醒间隔", "两次提醒之间的等待时间"))
            .child(interval);

        let title_bar = titlebar("设置", window_button("\u{e8bb}", WindowControlArea::Close));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(palette::WINDOW_BG))
            .text_color(rgb(palette::TEXT))
            .child(title_bar)
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
                            .text_color(rgb(palette::TEXT_MUTED))
                            .mb_2()
                            .child("提醒"),
                    )
                    .child(interval_row)
                    .child(div().h(px(12.)))
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(palette::TEXT_MUTED))
                            .mb_2()
                            .child("启动"),
                    )
                    .child(autostart),
            )
    }
}

fn setting_copy(title: &'static str, description: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_color(rgb(palette::TEXT)).child(title))
        .child(
            div()
                .text_sm()
                .text_color(rgb(palette::TEXT_MUTED))
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
            rgb(palette::ACCENT)
        } else {
            rgb(palette::SWITCH_OFF)
        })
        .child(
            div()
                .size(px(16.))
                .rounded_full()
                .bg(rgb(palette::WHITE))
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
            rgb(palette::TEXT)
        } else {
            rgb(palette::TEXT_DISABLED)
        })
        .bg(if enabled {
            rgb(palette::STEP_BG)
        } else {
            rgb(palette::STEP_BG_DISABLED)
        })
        .when(enabled, |this| {
            this.hover(|s| s.bg(rgb(palette::STEP_BG_HOVER)))
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
            window_bounds: Some(WindowBounds::centered(size(px(520.), px(420.)), cx)),
            titlebar: None,
            kind: WindowKind::Normal,
            is_resizable: false,
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
