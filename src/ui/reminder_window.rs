use crate::{
    core::{
        data::{last_time, save_time},
        paths::reminder_image_file,
        scheduler::{RescheduleType, SchedulerCmd},
    },
    ui::{format_clock_secs, format_day_label, format_span, image_format, local_date, now, palette},
};
use gpui_kit::{
    App, Context, FontWeight, Image, ImageFormat, MouseButton, Rems, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, img, prelude::*, rgb,
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
    /// 上一次点「喝了」是否写库失败。失败时不顺延 deadline，改为留在浮层上提示，
    /// 这是唯一能让用户察觉记录没保存的通道。
    save_failed: bool,
}

impl ReminderWindow {
    /// 浮层开着时用户可能改分辨率、拔接显示器，屏幕矩形会变，重新对齐一次。
    ///
    /// 订阅必须挂到 View 上（而不是开窗闭包里的 `window`）：`bounds` 观察者
    /// 需要 `Context<Self>`。且必须 `detach()`，否则 `Subscription` 在
    /// `new()` 返回时就被 drop，回调立刻注销。
    ///
    /// `refit_reminder_window` 是幂等的（已对齐就直接返回），所以它触发的
    /// `WM_MOVE` 再回调一次也不会递归。
    fn observe_monitor_changes(window: &mut Window, cx: &mut Context<Self>) {
        cx.observe_window_bounds(window, |_, window, _| {
            crate::platform::refit_reminder_window(window);
        })
        .detach();
    }

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
        // 浮层外壳恒为深色（见 `palette::overlay`）：整屏压暗才能让水杯插图与白字立住。
        // 但主按钮是唯一跟着界面走的地方 —— 用系统浅色下的深蓝，
        // 浅色外观时按钮仍是蓝底白字而不是浅得看不见。
        let drink = div()
            .px_10()
            .py_4()
            .rounded_md()
            .bg(rgb(palette::current().accent))
            .hover(|s| s.bg(rgb(palette::current().accent_hover)))
            .cursor_pointer()
            .text_color(rgb(palette::WHITE))
            .text_3xl()
            .child("喝了")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, w, cx| {
                    if save_time() {
                        let _ = tx.send(SchedulerCmd::Reschedule(RescheduleType::Drink));
                        w.remove_window();
                    } else {
                        // 记录没落盘：此时顺延提醒等于把丢失的打卡当成成功，
                        // 下一次提醒会按错误的时间点排期。保留浮层并提示，让用户重试。
                        this.save_failed = true;
                        cx.notify();
                    }
                }),
            );
        let skip = div()
            .px_10()
            .py_4()
            .rounded_md()
            .border_1()
            .border_color(rgb(palette::overlay::BORDER))
            .cursor_pointer()
            .text_color(rgb(palette::overlay::TEXT))
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
            .bg(palette::overlay::bg())
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
                    .text_color(rgb(palette::overlay::TEXT))
                    .text_size(Rems(1.05))
                    .child(status),
            )
            .when(self.save_failed, |this| {
                this.child(
                    div()
                        .mt_2()
                        .text_color(rgb(palette::overlay::WARNING))
                        .text_size(Rems(1.05))
                        .child("刚才的记录没能保存，请再点一次「喝了」"),
                )
            })
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
            move |window, cx| {
                crate::ui::follow_system_appearance(window);
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

                    // 这里已经能拿到 `Context<ReminderWindow>`，把 bounds 观察者挂上。
                    ReminderWindow::observe_monitor_changes(window, cx);

                    ReminderWindow {
                        scheduler: tx,
                        image,
                        last_drink,
                        next_reminder,
                        save_failed: false,
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
