use crate::{
    config::{Store, WindowState, save_store},
    data::load_timestamps,
    scheduler::AppCmd,
    settings_window::{close_settings_window, open_settings_window},
    ui::{
        calendar_color, format_clock, format_date, local_date, now, relative_to_now, window_button,
    },
};
use chrono::{Datelike, Duration as DateDuration, Local, NaiveDate};
use gpui::{
    App, Bounds, Context, MouseButton, Window, WindowBackgroundAppearance, WindowBounds,
    WindowControlArea, WindowKind, WindowOptions, div, point, prelude::*, px, rgb, size,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
};

pub struct MainWindow {
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<AppCmd>,
    selected_date: NaiveDate,
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let today = local_date(now());
        let selected = self.selected_date;
        let timestamps = load_timestamps();
        let selected_timestamps: Vec<u64> = timestamps
            .iter()
            .copied()
            .filter(|t| local_date(*t) == selected)
            .collect();
        let mut drink_counts = HashMap::new();
        for timestamp in &timestamps {
            *drink_counts.entry(local_date(*timestamp)).or_insert(0usize) += 1;
        }
        let earliest = timestamps
            .iter()
            .map(|t| local_date(*t))
            .min()
            .unwrap_or(today);
        let earliest_month_start = earliest.with_day(1).unwrap_or(earliest);
        let day_count = today
            .signed_duration_since(earliest_month_start)
            .num_days()
            .max(0) as usize
            + 1;
        let row_count = day_count.div_ceil(7).max(1);

        let mut calendar = div().flex().flex_col().gap_1().w(px(250.));
        for row in 0..row_count {
            let month = (0..7).find_map(|offset| {
                let day = today - DateDuration::days((row * 7 + offset) as i64);
                (day.day() == 1).then_some(day.month())
            });
            let mut week = div().flex().gap_1();
            for offset in 0..7 {
                let day = today - DateDuration::days((row * 7 + offset) as i64);
                let count = drink_counts.get(&day).copied().unwrap_or(0);
                let mut cell = div()
                    .w(px(24.))
                    .h(px(24.))
                    .rounded_sm()
                    .cursor_pointer()
                    .child("")
                    .bg(calendar_color(count));
                if day == selected {
                    cell = cell.border_2().border_color(rgb(0xffffff));
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
                .text_color(rgb(0x94a3b8))
                .child(
                    month
                        .map(|month| format!("{}月", month))
                        .unwrap_or_default(),
                );
            calendar = calendar.child(div().flex().items_center().child(month_label).child(week));
        }
        let calendar = calendar.mt_4();

        let mut records = div().flex().flex_col().gap_2().mt_3();
        if selected_timestamps.is_empty() {
            records = records.child(div().text_color(rgb(0x94a3b8)).child("这天没有喝水记录"));
        } else {
            for timestamp in selected_timestamps.iter().rev() {
                let text = if selected == today {
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
                        .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(rgb(0x60a5fa)))
                        .child(text),
                );
            }
        }
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
                    .child("喝水提醒"),
            )
            .child(
                div()
                    .id("settings-button")
                    .font_family("Segoe Fluent Icons")
                    .w(px(46.))
                    .h(px(38.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(14.))
                    .text_color(rgb(0xe0f2fe))
                    .hover(|s| s.bg(rgb(0x254c77)))
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
                                    .font_weight(gpui::FontWeight::BOLD)
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
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(format_date(selected)),
                            )
                            .child(div().mt_1().text_color(rgb(0x94a3b8)).child(
                                if selected == today {
                                    "今天"
                                } else {
                                    "历史记录"
                                },
                            ))
                            .child(records),
                    ),
            )
    }
}

pub fn open_main_window(cx: &mut App, store: Arc<Mutex<Store>>, scheduler: mpsc::Sender<AppCmd>) {
    if let Some(handle) = cx.windows().iter().find_map(|w| w.downcast::<MainWindow>()) {
        let _ = handle.update(cx, |_, window, _| {
            #[cfg(windows)]
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(value) = handle.as_raw() {
                    crate::platform::show_main_window(windows::Win32::Foundation::HWND(
                        value.hwnd.get() as *mut _,
                    ));
                }
            }
            window.activate_window();
        });
        return;
    }
    create_main_window(cx, store, scheduler, true);
}

pub fn save_main_window_state(cx: &mut App) {
    if let Some(handle) = cx.windows().iter().find_map(|w| w.downcast::<MainWindow>()) {
        let _ = handle.update(cx, |view, window, _| {
            let bounds = window.window_bounds();
            let (bounds, maximized) = match bounds {
                WindowBounds::Windowed(bounds) => (bounds, false),
                WindowBounds::Maximized(bounds) => (bounds, true),
                WindowBounds::Fullscreen(bounds) => (bounds, false),
            };
            if let Ok(mut store) = view.store.lock() {
                store.window_state = Some(WindowState {
                    x: bounds.origin.x.as_f32(),
                    y: bounds.origin.y.as_f32(),
                    width: bounds.size.width.as_f32(),
                    height: bounds.size.height.as_f32(),
                    maximized,
                });
                save_store(&store);
            }
        });
    }
}

fn create_main_window(
    cx: &mut App,
    store: Arc<Mutex<Store>>,
    scheduler: mpsc::Sender<AppCmd>,
    show: bool,
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
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("喝水提醒".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                kind: WindowKind::Normal,
                is_resizable: true,
                window_min_size: Some(size(px(500.), px(800.))),
                window_background: WindowBackgroundAppearance::Opaque,
                show: show || cfg!(not(windows)),
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
                    let bounds = window.window_bounds();
                    let (bounds, maximized) = match bounds {
                        WindowBounds::Windowed(bounds) => (bounds, false),
                        WindowBounds::Maximized(bounds) => (bounds, true),
                        WindowBounds::Fullscreen(bounds) => (bounds, false),
                    };
                    if let Ok(mut store) = close_store.lock() {
                        store.window_state = Some(WindowState {
                            x: bounds.origin.x.as_f32(),
                            y: bounds.origin.y.as_f32(),
                            width: bounds.size.width.as_f32(),
                            height: bounds.size.height.as_f32(),
                            maximized,
                        });
                        save_store(&store);
                    }
                    true
                });
                view
            },
        )
        .ok();
    #[cfg(windows)]
    if let Some(handle) = handle {
        let _ = handle.update(cx, |_, window, _| {
            if let Ok(h) = window.window_handle() {
                if let RawWindowHandle::Win32(v) = h.as_raw() {
                    crate::platform::style_main_window(windows::Win32::Foundation::HWND(
                        v.hwnd.get() as *mut _,
                    ));
                    if show {
                        crate::platform::show_main_window(windows::Win32::Foundation::HWND(
                            v.hwnd.get() as *mut _,
                        ));
                    }
                }
            }
        });
    }
}
