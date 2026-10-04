use crate::{
    core::{
        data::{last_time, save_time},
        paths::reminder_image_file,
        scheduler::{RescheduleType, SchedulerCmd},
    },
    ui::{format_clock_secs, format_day_label, format_span, image_format, local_date, now, palette},
};
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{
    App, Context, FontWeight, Image, ImageFormat, Rems, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, div, img, prelude::*, rgb,
};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};

pub struct ReminderWindow {
    scheduler: mpsc::Sender<SchedulerCmd>,
    /// 浮层中央的插图：自选图片或内置图，开窗时解析一次。
    image: Arc<Image>,
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
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let tx = self.scheduler.clone();
        let status = self.status_text();
        let drink = Button::new("drink")
            .label("喝了")
            .primary()
            .large()
            .on_click(move |_, window, _| {
                save_time();
                let _ = tx.send(SchedulerCmd::Reschedule(RescheduleType::Drink));
                window.remove_window();
            });
        let skip = Button::new("skip")
            .label("跳过")
            .outline()
            .large()
            .on_click(|_, window, _| window.remove_window());
        div()
            .flex()
            .flex_col()
            .size_full()
            .justify_start()
            .items_center()
            .gap_2()
            .bg(palette::overlay_bg())
            .child(
                div()
                    .text_color(rgb(palette::WHITE))
                    .text_size(Rems(2.5))
                    .font_weight(FontWeight::BOLD)
                    .child("该喝水了")
                    .mt_24(),
            )
            .child(
                div()
                    .mt_2()
                    .text_color(rgb(palette::TEXT_SOFT))
                    .text_size(Rems(1.05))
                    .child(status),
            )
            .child(img(self.image.clone()).size_128())
            .child(div().flex().gap_12().mt_4().child(drink).child(skip))
    }
}

/// 浮层插图：数据目录下的 `reminder.img` 存在且可解析时优先使用，否则用内置 `water.png`。
fn reminder_image() -> Arc<Image> {
    let custom = std::fs::read(reminder_image_file())
        .ok()
        .and_then(|bytes| image_format(&bytes).map(|format| (format, bytes)));
    match custom {
        Some((format, bytes)) => Arc::new(Image::from_bytes(format, bytes)),
        None => Arc::new(Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../assets/water.png").to_vec(),
        )),
    }
}

pub fn open_reminder_window(cx: &mut App, tx: mpsc::Sender<SchedulerCmd>, remaining: u64) {
    let last_drink = last_time();
    let next_reminder = now() + remaining;

    // 已经有浮层时不能只是默默返回：浮层可能在屏幕还没亮、显示器正在重枚举、渲染
    // 设备刚丢失的那个瞬间被创建出来，用户根本看不见它，而它除了被点击「喝了」/
    // 「跳过」之外不会被自动关闭 —— 一旦如此，此后每一次提醒都会被这里吞掉，直到
    // 重启进程。所以改为原地更新两份状态并重绘。
    if let Some(handle) = cx
        .windows()
        .iter()
        .find_map(|w| w.downcast::<ReminderWindow>())
    {
        let _ = handle.update(cx, |view, window, cx| {
            view.last_drink = last_drink;
            view.next_reminder = next_reminder;
            window.refresh();
            cx.notify();
        });
        return;
    }

    let display = match cx.primary_display() {
        Some(d) => d,
        None => return,
    };
    let bounds = display.bounds();
    let image = reminder_image();
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
                        image,
                        last_drink,
                        next_reminder,
                    }
                })
            },
        )
        .ok();
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| {
            crate::platform::style_reminder_window(window)
        });
    }
}
