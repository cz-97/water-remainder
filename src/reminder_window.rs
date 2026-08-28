use crate::{
    config::{Store, load_store, save_store},
    scheduler::AppCmd,
    ui::now,
};
use gpui::{
    App, Context, Image, ImageFormat, MouseButton, Rems, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, hsla, img, prelude::*, rgb,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub struct ReminderWindow {
    scheduler: mpsc::Sender<AppCmd>,
    store: Arc<Mutex<Store>>,
}
impl Render for ReminderWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tx = self.scheduler.clone();
        let store = self.store.clone();
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
                    if let Ok(mut s) = store.lock() {
                        s.drinks.push(now());
                        save_store(&s);
                    }
                    let _ = tx.send(AppCmd::Reset(Duration::from_secs(
                        load_store().settings.interval_secs,
                    )));
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
                    .child("该喝水了 💧")
                    .mt_32(),
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
pub fn open_reminder_window(cx: &mut App, tx: mpsc::Sender<AppCmd>, store: Arc<Mutex<Store>>) {
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
                    store,
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
