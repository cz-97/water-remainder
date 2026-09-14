use crate::{
    data::{last_time, save_time},
    scheduler::{AppCmd, RescheduleType},
    ui::{format_clock_secs, format_day_label, format_span, local_date, now},
};
use gpui::{
    App, Context, Image, ImageFormat, MouseButton, Rems, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, hsla, img, prelude::*, rgb,
};
use std::{sync::mpsc, time::Duration};
pub struct ReminderWindow {
    scheduler: mpsc::Sender<AppCmd>,
    /// 最近一次喝水的时间戳（Unix 秒）。
    last_drink: Option<u64>,
    /// 下一次提醒的时刻（Unix 秒）。
    next_reminder: u64,
}

impl ReminderWindow {
    /// 两句话都在 render 里按「当前时刻」现算，因此每秒重绘一次就能按秒跳动。
    fn status_text(&self) -> String {
        let current = now();
        let today = local_date(current);

        let last_part = match self.last_drink {
            Some(last) => format!(
                "您在 {}前喝过水（{}{}）",
                format_span(current.saturating_sub(last)),
                format_day_label(local_date(last), today),
                format_clock_secs(last)
            ),
            None => "您还没有喝水记录".to_string(),
        };
        let next_part = format!(
            "将于 {}后再次提醒您（{}）",
            format_span(self.next_reminder.saturating_sub(current)),
            format_clock_secs(self.next_reminder)
        );

        format!("{}，{}", last_part, next_part)
    }
}

impl Render for ReminderWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tx = self.scheduler.clone();
        let status = self.status_text();
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
                    let _ = tx.send(AppCmd::Reschedule(RescheduleType::Drink));
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
                    .child(status),
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
pub fn open_reminder_window(cx: &mut App, tx: mpsc::Sender<AppCmd>, remaining: u64) {
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
    let last_drink = last_time();
    let next_reminder = now() + remaining;
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
                cx.new(|cx| {
                    // 每秒 notify 一次触发重绘，让两处倒计时按秒跳动。
                    // 窗口关闭后弱引用升级失败，任务自行退出。
                    cx.spawn(async move |this, cx| {
                        loop {
                            cx.background_executor().timer(Duration::from_secs(1)).await;
                            if this.update(cx, |_, cx| cx.notify()).is_err() {
                                break;
                            }
                        }
                    })
                    .detach();

                    ReminderWindow {
                        scheduler: tx,
                        last_drink,
                        next_reminder,
                    }
                })
            },
        )
        .ok();
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| crate::platform::style_reminder_window(window));
    }
}
