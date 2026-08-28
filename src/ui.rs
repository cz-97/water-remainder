use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use gpui::{WindowControlArea, div, prelude::*, px, rgb};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn local_date(timestamp: u64) -> NaiveDate {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|d| d.with_timezone(&Local).date_naive())
        .unwrap_or_else(|| Local::now().date_naive())
}
pub fn format_clock(timestamp: u64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|d| d.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".into())
}
pub fn relative_to_now(timestamp: u64) -> String {
    let seconds = now().saturating_sub(timestamp);
    if seconds < 60 {
        "刚刚".into()
    } else if seconds < 3600 {
        format!("{} 分钟前", seconds / 60)
    } else {
        format!("{} 小时前", seconds / 3600)
    }
}
pub fn format_date(date: NaiveDate) -> String {
    format!("{}年{}月{}日", date.year(), date.month(), date.day())
}

pub fn calendar_color(count: usize) -> gpui::Rgba {
    match count {
        0 => rgb(0x404040),
        1..=3 => rgb(0xdbeafe),
        4..=6 => rgb(0xbfdbfe),
        7..=9 => rgb(0x93c5fd),
        10..=12 => rgb(0x60a5fa),
        13..=15 => rgb(0x3b82f6),
        16..=18 => rgb(0x2563eb),
        19..=21 => rgb(0x1d4ed8),
        _ => rgb(0x1e3a8a),
    }
}

pub fn window_button(label: &'static str, area: WindowControlArea) -> impl IntoElement {
    let hover = if area == WindowControlArea::Close {
        rgb(0xdc2626)
    } else {
        rgb(0x254c77)
    };
    let id = match area {
        WindowControlArea::Min => "window-minimize",
        WindowControlArea::Max => "window-maximize",
        WindowControlArea::Close => "window-close",
        WindowControlArea::Drag => "window-drag",
    };
    div()
        .id(id)
        .font_family("Segoe Fluent Icons")
        .w(px(46.))
        .h(px(38.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(18.))
        .text_color(rgb(0xe0f2fe))
        .occlude()
        .hover(move |s| s.bg(hover))
        .window_control_area(area)
        .child(label)
}
