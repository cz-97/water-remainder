use crate::{
    core::{
        config::{INTERVALS, Store, update_settings},
        paths::reminder_image_file,
        scheduler::{RescheduleType, SchedulerCmd},
    },
    platform,
    ui::{image_format, palette, titlebar, window_button},
};
use gpui_kit::component::{
    Disableable,
    button::{Button, ButtonVariants},
    switch::Switch,
};
use gpui_kit::{
    App, Context, Window, WindowBounds, WindowControlArea, WindowKind, WindowOptions, div,
    prelude::*, px, rgb, size,
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
        let interval_index = INTERVALS
            .iter()
            .position(|seconds| *seconds == settings.interval_secs)
            .unwrap_or(0);
        let can_decrease = interval_index > 0;
        let can_increase = interval_index + 1 < INTERVALS.len();

        let decrease_index = interval_index.saturating_sub(1);
        let decrease = Button::new("interval-decrease")
            .label("−")
            .ghost()
            .disabled(!can_decrease)
            .on_click(cx.listener(move |this, _, _, cx| {
                set_interval(&this.store, &this.scheduler, decrease_index);
                cx.notify();
            }));
        let increase_index = interval_index + 1;
        let increase = Button::new("interval-increase")
            .label("+")
            .ghost()
            .disabled(!can_increase)
            .on_click(cx.listener(move |this, _, _, cx| {
                set_interval(&this.store, &this.scheduler, increase_index);
                cx.notify();
            }));
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
        let interval_row = row(setting_copy("提醒间隔", "两次提醒之间的等待时间"), interval);

        let autostart = Switch::new("autostart")
            .checked(settings.autostart)
            .on_change(cx.listener(|this, checked, _, cx| {
                let enabled = update_settings(&this.store, |settings| {
                    settings.autostart = *checked;
                    settings.autostart
                });
                if let Some(enabled) = enabled {
                    platform::set_autostart(enabled);
                }
                cx.notify();
            }));
        let autostart_row = row(
            setting_copy("开机启动", "登录 Windows 后自动启动喝水提醒"),
            autostart,
        );

        let custom = reminder_image_file().exists();
        let change = Button::new("change-image")
            .label("更换")
            .outline()
            .on_click(cx.listener(|_, _, _, cx| {
                if let Some(path) = platform::pick_image_file() {
                    if let Ok(bytes) = fs::read(&path) {
                        if image_format(&bytes).is_some() {
                            let _ = fs::write(reminder_image_file(), &bytes);
                        }
                    }
                }
                cx.notify();
            }));
        let reset = Button::new("reset-image")
            .label("恢复默认")
            .outline()
            .disabled(!custom)
            .on_click(cx.listener(|_, _, _, cx| {
                let _ = fs::remove_file(reminder_image_file());
                cx.notify();
            }));
        let image_row = row(
            setting_copy("提醒图片", "提醒浮层中央的插图，可替换为本地图片"),
            div().flex().items_center().gap_2().child(change).child(reset),
        );

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
                    .child(section_title("提醒"))
                    .child(interval_row)
                    .child(image_row)
                    .child(div().h(px(12.)))
                    .child(section_title("启动"))
                    .child(autostart_row),
            )
    }
}

fn section_title(text: &'static str) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(rgb(palette::TEXT_MUTED))
        .mb_2()
        .child(text)
}

fn row(left: impl IntoElement, right: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .px_4()
        .py_3()
        .rounded_md()
        .child(left)
        .child(right)
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
