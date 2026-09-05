use crate::{
    data::{get_last_time, save_time},
    scheduler::AppCmd,
    ui::now,
};
use gpui::{
    App, Context, Image, ImageFormat, MouseButton, Rems, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, hsla, img, prelude::*, rgb,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::mpsc;
use std::time::Duration;

pub struct ReminderWindow {
    scheduler: mpsc::Sender<AppCmd>,
    reminder_status: String,
}
impl Render for ReminderWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tx = self.scheduler.clone();
        let drink = div()
            .px_10()
            .py_4()
            .rounded_md()
            .bg(rgb(0x60a5fa))
            .hover(|s| s.bg(rgb(0x3b82f6)))
            .cursor_pointer()
            .text_color(rgb(0xffffff))
            .text_3xl()
            .child("喝了")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, w, _| {
                    save_time();
                    let _ = tx.send(AppCmd::Reset);
                    w.remove_window();
                }),
            );
        let skip = div()
            .px_10()
            .py_4()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x64748b))
            .cursor_pointer()
            .text_color(rgb(0xe2e8f0))
            .text_3xl()
            .child("跳过")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, w, _| w.remove_window()),
            );
        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_start()
            .items_center()
            .gap_2()
            .bg(hsla(0., 0., 0., 0.85))
            .child(
                div()
                    .text_color(rgb(0xffffff))
                    .text_size(Rems(2.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("该喝水了")
                    .mt_24(),
            )
            .child(
                div()
                    .mt_2()
                    .text_color(rgb(0xcbd5e1))
                    .text_size(Rems(1.05))
                    .child(self.reminder_status.clone()),
            )
            .child(
                img(std::sync::Arc::new(Image::from_bytes(
                    ImageFormat::Png,
                    include_bytes!("assets/water.png").to_vec(),
                )))
                .size_128(),
            )
            .child(div().flex().gap_12().mt_4().child(drink).child(skip))
    }
}
pub fn open_reminder_window(cx: &mut App, tx: mpsc::Sender<AppCmd>, remaining: Duration) {
    if cx
        .windows()
        .iter()
        .any(|w| w.downcast::<ReminderWindow>().is_some())
    {
        return;
    }
    let display = match cx.primary_display() {
        Some(d) => d,
        None => return,
    };
    let bounds = display.bounds();
    let reminder_status = reminder_status(now(), remaining);
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                window_background: WindowBackgroundAppearance::Transparent,
                kind: WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                display_id: Some(display.id()),
                ..Default::default()
            },
            move |_, cx| {
                cx.new(|_| ReminderWindow {
                    scheduler: tx,
                    reminder_status,
                })
            },
        )
        .ok();
    #[cfg(windows)]
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| {
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(value) = handle.as_raw() {
                    crate::platform::strip_win11_chrome(windows::Win32::Foundation::HWND(
                        value.hwnd.get() as *mut _,
                    ));
                }
            }
        });
    }
}

fn reminder_status(opened_at: u64, remaining: Duration) -> String {
    let remaining_minutes = remaining.as_secs().div_ceil(60);
    let Some(last_drink) = get_last_time() else {
        return format!(
            "您还没有喝水记录，将于{}分钟后再次提醒您",
            remaining_minutes
        );
    };

    let elapsed_secs = opened_at.saturating_sub(last_drink);
    let elapsed_minutes = elapsed_secs / 60;
    format!(
        "您在{}分钟前喝过水，将于{}分钟后再次提醒您",
        elapsed_minutes, remaining_minutes
    )
}
