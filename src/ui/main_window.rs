use crate::{
    core::{
        config::{Store, WindowState, save_store},
        data::{DayCounts, day_detail, earliest_day, month_counts},
        scheduler::SchedulerCmd,
        time::{format_clock, format_date, local_date, now, relative_to_now},
    },
    ui::{
        ACTION_ICON_SIZE, calendar_color, calendar_level, palette,
        settings_window::{close_settings_window, open_settings_window},
        titlebar_button, titlebar_split, window_button,
    },
};
use chrono::{Datelike, Local, NaiveDate};
use gpui_kit::{
    App, Bounds, Context, Div, FontWeight, MouseButton, Stateful, TitlebarOptions, Window,
    WindowBounds, WindowControlArea, WindowKind, WindowOptions, div, point, prelude::*, px, rgb,
    size,
};
use std::sync::{Arc, Mutex, mpsc};

const CALENDER_WIDTH: f32 = 250.;
const TIMELINE_WIDTH: f32 = 164.;
/// 内容区左右内边距（`p_6` = 1.5rem = 24px，两侧共 48px）。
const CONTENT_PADDING: f32 = 48.;
/// 两列之间的间距（`gap_12` = 3rem = 48px）。
const COLUMN_GAP: f32 = 48.;

const CONTENT_WIDTH: f32 = CALENDER_WIDTH + TIMELINE_WIDTH + CONTENT_PADDING + COLUMN_GAP;

/// 主窗最小高度。
const CONTENT_HEIGHT: f32 = 800.;

/// 把恢复出来的窗口尺寸抬到下限以上（只放大、不缩小）。
///
/// 需要这一步是因为 `window_min_size` 只作用在**用户拖拽**上（走 `WM_GETMINMAXINFO`
/// 的最小跟踪尺寸），并不校验程序自己传给 `CreateWindowExW` 的初始尺寸。旧版本把
/// 最小宽度设成 500，于是 `settings.txt` 里存下的就是 500 —— 直接按原样恢复的话，
/// 修好最小宽度也没用，用户看到的仍是被压扁的两列。等到用户真正拖动一次窗口，
/// 新值才会写回设置文件。
fn clamp_to_minimum(width: f32, height: f32) -> (f32, f32) {
    (width.max(CONTENT_WIDTH), height.max(CONTENT_HEIGHT))
}

/// 一次渲染的全部外部输入。显式传递，「当前时刻」与「记录」因此只在这一处取得，
/// 子视图不再各自去读全局状态。
struct RenderInput {
    today: NaiveDate,
    selected: NaiveDate,
    /// 全局最早有记录的日期，用于左箭头下界（与按月聚合解耦）。
    earliest: Option<NaiveDate>,
    /// 当前展示月份的日级聚合：日历上色只需要当月每天的次数。
    counts: Arc<DayCounts>,
    /// 选中那天的明细，懒加载的结果（次数为 0 时为空）。
    detail: Arc<Vec<u64>>,
}

pub struct MainWindow {
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<SchedulerCmd>,
    selected_date: NaiveDate,
    /// 日历当前展示的月份（该月 1 号）。左右箭头切换，初始为当月。
    view_month: NaiveDate,
}

// 三个子视图都返回具体的 `Div` 而非 `impl IntoElement`：后者会让返回值
// 隐式借用 `&mut Context`，同一个 `render` 里就无法再把它交给别的子视图。
impl MainWindow {
    /// 月视图日历：标题为「年 月」，左右箭头切换月份。每格是当月某天，
    /// 背景按次数上色（0 次为色阶底色），选中日加白框。
    fn calendar(&self, input: &RenderInput, cx: &mut Context<Self>) -> Div {
        let month = self.view_month;
        let days = days_in_month(month);
        // 周一为每周首列：0 = 周一。
        let leading = month.weekday().num_days_from_monday();
        // 左箭头：逐月后退，退到最早有记录的月份为止，再往前则置灰。
        let earliest_month = input.earliest.map(|day| day.with_day(1).unwrap_or(day));
        let prev_month = match earliest_month {
            Some(earliest) if month > earliest => Some(shift_month(month, -1)),
            _ => None,
        };
        // 右箭头：逐月前进，不允许翻到未来。
        let next_month =
            (month < input.today.with_day(1).unwrap_or(input.today)).then(|| shift_month(month, 1));

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(self.month_nav_button("month-prev", "\u{2039}", prev_month, cx))
            .child(div().text_lg().font_weight(FontWeight::BOLD).child(format!(
                "{}年{}月",
                month.year(),
                month.month()
            )))
            .child(self.month_nav_button("month-next", "\u{203a}", next_month, cx));

        let mut weekday_header = div().flex().gap_1();
        for label in ["一", "二", "三", "四", "五", "六", "日"] {
            weekday_header = weekday_header.child(
                div()
                    .w(px(30.))
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(rgb(palette::current().text_muted))
                    .child(label),
            );
        }

        let mut grid = div().flex().flex_col().gap_1();
        let total_cells = (leading + days).div_ceil(7) * 7;
        for row in 0..total_cells / 7 {
            let mut week = div().flex().gap_1();
            for col in 0..7 {
                let index = row * 7 + col;
                if index < leading || index >= leading + days {
                    week = week.child(div().w(px(30.)).h(px(30.)));
                    continue;
                }
                let day = month.with_day(index - leading + 1).unwrap_or(month);
                let count = input.counts.count(day);
                let level = calendar_level(count);
                let mut cell = div()
                    .w(px(30.))
                    .h(px(30.))
                    .rounded_sm()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .cursor_pointer()
                    .bg(calendar_color(count))
                    .text_color(rgb(palette::calendar_text(level)))
                    .child(format!("{}", day.day()));
                if day == input.selected {
                    cell = cell
                        .border_2()
                        .border_color(rgb(palette::current().selection_ring));
                }
                week = week.child(cell.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.selected_date = day;
                        cx.notify();
                    }),
                ));
            }
            grid = grid.child(week);
        }

        div()
            .w(px(CALENDER_WIDTH))
            .flex()
            .flex_col()
            .gap_2()
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(weekday_header)
                    .child(grid),
            )
            .mt_4()
    }

    /// 月份切换箭头。`target = None` 时置灰且不响应点击。
    fn month_nav_button(
        &self,
        id: &'static str,
        label: &'static str,
        target: Option<NaiveDate>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let button = div()
            .id(id)
            .w(px(28.))
            .h(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .text_xl()
            .text_color(rgb(match target {
                Some(_) => palette::current().text_soft,
                None => palette::current().text_disabled,
            }))
            .child(label);
        match target {
            Some(target) => button
                .cursor_pointer()
                .hover(|style| style.bg(rgb(palette::current().row_hover)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.view_month = target;
                        cx.notify();
                    }),
                ),
            None => button,
        }
    }

    /// 右侧时间轴：只渲染选中日的明细，明细本身是懒加载的结果（借用 `Arc`，不拷贝）。
    fn timeline(&self, input: &RenderInput) -> Div {
        let timestamps = input.detail.as_slice();
        let mut records = div().flex().flex_col().gap_2().mt_3();
        if timestamps.is_empty() {
            records = records.child(
                div()
                    .text_color(rgb(palette::current().text_muted))
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
                                .bg(rgb(palette::current().accent)),
                        )
                        .child(text),
                );
            }
        }
        records
    }

    /// 标题栏左侧：设置 + 回到今天。两个键共用 `titlebar_button` 的 46×38 样式，
    /// 所以高度、垂直居中与彼此的间距天然一致，不需要额外对齐。
    fn title_leading_actions(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .child(
                titlebar_button(
                    "settings-button",
                    palette::current().titlebar_hover,
                    palette::current().titlebar_fg,
                    ACTION_ICON_SIZE,
                )
                .cursor_pointer()
                .child("\u{e713}")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        open_settings_window(cx, this.store.clone(), this.scheduler.clone());
                    }),
                ),
            )
            .child(
                titlebar_button(
                    "today-button",
                    palette::current().titlebar_hover,
                    palette::current().titlebar_fg,
                    ACTION_ICON_SIZE,
                )
                .cursor_pointer()
                // E72C = Segoe MDL2/Fluent 的「刷新」。已经在当月且选中当天时
                // 没必要再动，但重绘成本只是两次 BTreeMap 查找，直接无条件重置。
                .child("\u{e72c}")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        let today = local_date(now());
                        this.selected_date = today;
                        this.view_month = today.with_day(1).unwrap_or(today);
                        cx.notify();
                    }),
                ),
            )
    }

    /// 标题栏右侧：最小化 / 最大化 / 关闭。顺序与语义都不能动。
    fn title_trailing_actions(&self, window: &Window) -> Div {
        div()
            .flex()
            .items_center()
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
            earliest: earliest_day(),
            counts: month_counts(self.view_month),
            detail: day_detail(self.selected_date),
        };
        let calendar = self.calendar(&input, cx);
        let timeline = self.timeline(&input);
        let title_bar = titlebar_split(
            self.title_leading_actions(cx),
            "喝水统计",
            self.title_trailing_actions(window),
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(palette::current().window_bg))
            .text_color(rgb(palette::current().text))
            .child(title_bar)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .p_6()
                    .gap_12()
                    .justify_center()
                    .child(
                        div()
                            .w(px(CALENDER_WIDTH))
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
                            .w(px(TIMELINE_WIDTH))
                            .flex()
                            .flex_col()
                            .child(div().text_xl().font_weight(FontWeight::BOLD).child(
                                match input.selected == input.today {
                                    true => "今天",
                                    false => "历史记录",
                                },
                            ))
                            .child(
                                div()
                                    .mt_1()
                                    .text_color(rgb(palette::current().text_muted))
                                    .child(format_date(input.selected)),
                            )
                            .child(timeline),
                    ),
            )
    }
}

/// 月份平移：`month` 为该月 1 号，`delta` 以月为单位（可正可负）。
fn shift_month(month: NaiveDate, delta: i32) -> NaiveDate {
    let total = month.year() * 12 + month.month0() as i32 + delta;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1)
        .unwrap_or(month)
}

/// 当月天数：`month` 为该月 1 号。
fn days_in_month(month: NaiveDate) -> u32 {
    shift_month(month, 1)
        .signed_duration_since(month)
        .num_days() as u32
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
                // 窗口位置/尺寸属于「记不住也不影响使用」的一类：**刻意**只做尽力而为，
                // 失败不弹框打扰用户（关窗时弹一个「窗口位置没存住」毫无意义）。
                // 真正要紧的 interval / autostart 走设置窗，那里失败会明确提示。
                let _ = save_store(&store);
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
            // 存储的尺寸可能来自旧版本（当时最小宽度是 500），必须抬到下限，
            // 否则修好最小宽度也看不到效果。
            let (width, height) = clamp_to_minimum(s.width, s.height);
            let bounds = Bounds {
                origin: point(px(s.x), px(s.y)),
                size: size(px(width), px(height)),
            };
            if s.maximized {
                WindowBounds::Maximized(bounds)
            } else {
                WindowBounds::Windowed(bounds)
            }
        })
        .unwrap_or_else(|| WindowBounds::centered(size(px(CONTENT_WIDTH), px(CONTENT_HEIGHT)), cx));
    let close_store = store.clone();
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                titlebar: Some(TitlebarOptions {
                    title: Some("喝水统计".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                kind: WindowKind::Normal,
                window_min_size: Some(size(px(CONTENT_WIDTH), px(CONTENT_HEIGHT))),
                ..Default::default()
            },
            move |window, cx| {
                let close_store = close_store.clone();
                let view = cx.new(|_| MainWindow {
                    store,
                    scheduler,
                    selected_date: today,
                    view_month: today.with_day(1).unwrap_or(today),
                });
                crate::ui::follow_system_appearance(window);
                window.on_window_should_close(cx, move |window, cx| {
                    close_settings_window(cx);
                    if let Ok(mut store) = close_store.lock() {
                        store.window_state = Some(capture_window_state(window));
                        // 同 `save_main_window_state`：窗口状态存不住不值得打断用户，
                        // 窗口照常隐藏、进程照常常驻托盘。
                        let _ = save_store(&store);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 恢复旧窗口尺寸时抬到下限，并保留已足够大的尺寸。
    #[test]
    fn restored_size_is_clamped_to_minimum() {
        for (input, expected) in [
            ((500., 821.33), (CONTENT_WIDTH, 821.33)),
            ((400., 300.), (CONTENT_WIDTH, CONTENT_HEIGHT)),
            ((1200., 900.), (1200., 900.)),
        ] {
            assert_eq!(clamp_to_minimum(input.0, input.1), expected);
        }
    }
}
