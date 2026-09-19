use crate::{
    config::{Store, WindowState, save_store},
    data::{DayCounts, day_counts, day_detail},
    scheduler::SchedulerCmd,
    settings_window::{close_settings_window, open_settings_window},
    ui::{
        ACTION_ICON_SIZE, calendar_color, format_clock, format_date, local_date, now, palette,
        relative_to_now, titlebar, titlebar_button, window_button,
    },
};
use chrono::{Datelike, Duration as DateDuration, Local, NaiveDate};
use gpui_kit::{
    App, Bounds, Context, Div, FontWeight, MouseButton, TitlebarOptions, Window, WindowBounds,
    WindowControlArea, WindowKind, WindowOptions, div, point, prelude::*, px, rgb, size,
};
use std::sync::{Arc, Mutex, mpsc};

/// 一次渲染的全部外部输入。显式传递，「当前时刻」与「记录」因此只在这一处取得，
/// 子视图不再各自去读全局状态。
struct RenderInput {
    today: NaiveDate,
    selected: NaiveDate,
    /// 天级聚合：日历上色只需要每天的次数。
    counts: Arc<DayCounts>,
    /// 选中那天的明细，懒加载的结果（次数为 0 时为空）。
    detail: Arc<Vec<u64>>,
}

pub struct MainWindow {
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<SchedulerCmd>,
    selected_date: NaiveDate,
}

// 三个子视图都返回具体的 `Div` 而非 `impl IntoElement`：后者会让返回值
// 隐式借用 `&mut Context`，同一个 `render` 里就无法再把它交给别的子视图。
impl MainWindow {
    /// 日历热力图：只用到每天的条数，不碰明细。
    fn calendar(&self, input: &RenderInput, cx: &mut Context<Self>) -> Div {
        let earliest = input.counts.earliest_day().unwrap_or(input.today);
        let earliest_month_start = earliest.with_day(1).unwrap_or(earliest);
        let day_count = input
            .today
            .signed_duration_since(earliest_month_start)
            .num_days()
            .max(0) as usize
            + 1;
        let row_count = day_count.div_ceil(7).max(1);

        let mut calendar = div().flex().flex_col().gap_1().w(px(250.));
        for row in 0..row_count {
            let month = (0..7).find_map(|offset| {
                let day = input.today - DateDuration::days((row * 7 + offset) as i64);
                (day.day() == 1).then_some(day.month())
            });
            let mut week = div().flex().gap_1();
            for offset in 0..7 {
                let day = input.today - DateDuration::days((row * 7 + offset) as i64);
                let count = input.counts.count(day);
                let mut cell = div()
                    .w(px(24.))
                    .h(px(24.))
                    .rounded_sm()
                    .cursor_pointer()
                    .child("")
                    .bg(calendar_color(count));
                if day == input.selected {
                    cell = cell.border_2().border_color(rgb(palette::WHITE));
                }
                week = week.child(cell.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.selected_date = day;
                        cx.notify();
                    }),
                ));
            }
            let month_label = div()
                .w(px(36.))
                .h(px(24.))
                .flex()
                .items_center()
                .text_color(rgb(palette::TEXT_MUTED))
                .child(
                    month
                        .map(|month| format!("{}月", month))
                        .unwrap_or_default(),
                );
            calendar = calendar.child(div().flex().items_center().child(month_label).child(week));
        }
        calendar.mt_4()
    }

    /// 右侧时间轴：只渲染选中日的明细，明细本身是懒加载的结果（借用 `Arc`，不拷贝）。
    fn timeline(&self, input: &RenderInput) -> Div {
        let timestamps = input.detail.as_slice();
        let mut records = div().flex().flex_col().gap_2().mt_3();
        if timestamps.is_empty() {
            records = records.child(
                div()
                    .text_color(rgb(palette::TEXT_MUTED))
                    .child("这天没有喝水记录"),
            );
        } else {
            for timestamp in timestamps.iter().rev() {
                let text = if input.selected == input.today {
                    format!(
                        "{}  ·  {}",
                        format_clock(*timestamp),
                        relative_to_now(*timestamp)
                    )
                } else {
                    format_clock(*timestamp)
                };
                records = records.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .w(px(8.))
                                .h(px(8.))
                                .rounded_full()
                                .bg(rgb(palette::ACCENT)),
                        )
                        .child(text),
                );
            }
        }
        records
    }

    /// 标题栏右侧：设置入口 + 最小化 / 最大化 / 关闭。
    fn title_actions(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .child(
                titlebar_button("settings-button", palette::TITLEBAR_HOVER, ACTION_ICON_SIZE)
                    .cursor_pointer()
                    .child("\u{e713}")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            open_settings_window(cx, this.store.clone(), this.scheduler.clone());
                        }),
                    ),
            )
            .child(window_button("\u{e921}", WindowControlArea::Min))
            .child(window_button(
                if window.is_maximized() {
                    "\u{e923}"
                } else {
                    "\u{e922}"
                },
                WindowControlArea::Max,
            ))
            .child(window_button("\u{e8bb}", WindowControlArea::Close))
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 记录分两层：日历只用天级聚合（首次访问读一次库），明细只在选中那天取一次，
        // 且那天次数为 0 时连库都不碰。
        let input = RenderInput {
            today: local_date(now()),
            selected: self.selected_date,
            counts: day_counts(),
            detail: day_detail(self.selected_date),
        };
        let calendar = self.calendar(&input, cx);
        let timeline = self.timeline(&input);
        let title_bar = titlebar("喝水提醒", self.title_actions(window, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(palette::WINDOW_BG))
            .text_color(rgb(palette::TEXT))
            .child(title_bar)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .p_6()
                    .gap_8()
                    .justify_center()
                    .child(
                        div()
                            .w(px(250.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("喝水记录"),
                            )
                            .child(calendar),
                    )
                    .child(
                        div()
                            .w(px(250.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child(format_date(input.selected)),
                            )
                            .child(div().mt_1().text_color(rgb(palette::TEXT_MUTED)).child(
                                if input.selected == input.today {
                                    "今天"
                                } else {
                                    "历史记录"
                                },
                            ))
                            .child(timeline),
                    ),
            )
    }
}

/// 当前窗口的位置/尺寸/最大化状态，落盘与关闭时共用同一份采集逻辑。
fn capture_window_state(window: &Window) -> WindowState {
    let (bounds, maximized) = match window.window_bounds() {
        WindowBounds::Windowed(bounds) => (bounds, false),
        WindowBounds::Maximized(bounds) => (bounds, true),
        WindowBounds::Fullscreen(bounds) => (bounds, false),
    };
    WindowState {
        x: bounds.origin.x.as_f32(),
        y: bounds.origin.y.as_f32(),
        width: bounds.size.width.as_f32(),
        height: bounds.size.height.as_f32(),
        maximized,
    }
}

pub fn open_main_window(
    cx: &mut App,
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<SchedulerCmd>,
) {
    if let Some(handle) = cx.windows().iter().find_map(|w| w.downcast::<MainWindow>()) {
        let _ = handle.update(cx, |_, window, _| {
            crate::platform::show_main_window(window);
            window.activate_window();
        });
        return;
    }
    create_main_window(cx, store, scheduler);
}

pub fn save_main_window_state(cx: &mut App) {
    if let Some(handle) = cx.windows().iter().find_map(|w| w.downcast::<MainWindow>()) {
        let _ = handle.update(cx, |view, window, _| {
            if let Ok(mut store) = view.store.lock() {
                store.window_state = Some(capture_window_state(window));
                save_store(&store);
            }
        });
    }
}

fn create_main_window(
    cx: &mut App,
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<SchedulerCmd>,
) {
    let today = Local::now().date_naive();
    let saved_state = store.lock().ok().and_then(|s| s.window_state);
    let window_bounds = saved_state
        .map(|s| {
            let bounds = Bounds {
                origin: point(px(s.x), px(s.y)),
                size: size(px(s.width), px(s.height)),
            };
            if s.maximized {
                WindowBounds::Maximized(bounds)
            } else {
                WindowBounds::Windowed(bounds)
            }
        })
        .unwrap_or_else(|| WindowBounds::centered(size(px(500.), px(800.)), cx));
    let close_store = store.clone();
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                titlebar: Some(TitlebarOptions {
                    title: Some("喝水提醒".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                kind: WindowKind::Normal,
                window_min_size: Some(size(px(500.), px(800.))),
                ..Default::default()
            },
            move |window, cx| {
                let close_store = close_store.clone();
                let view = cx.new(|_| MainWindow {
                    store,
                    scheduler,
                    selected_date: today,
                });
                window.on_window_should_close(cx, move |window, cx| {
                    close_settings_window(cx);
                    if let Ok(mut store) = close_store.lock() {
                        store.window_state = Some(capture_window_state(window));
                        save_store(&store);
                    }
                    // 关闭按钮只是把窗口收起来，进程继续常驻托盘（退出走托盘菜单）。
                    // 返回 false 让 gpui 吞掉 WM_CLOSE，不再交给 DefWindowProc 销毁窗口。
                    crate::platform::hide_main_window(window);
                    false
                });
                view
            },
        )
        .ok();
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| {
            crate::platform::style_main_window(window);
            crate::platform::show_main_window(window);
        });
    }
}
