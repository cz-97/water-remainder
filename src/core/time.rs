//! 时间工具：时间戳换算、本地日期区间、以及给人看的文案格式化。
//!
//! 这里只有 chrono 与 `std::time`，没有任何 UI 依赖 —— `core::data` 需要按本地日
//! 切分查询区间，`ui` 需要把时间戳写成文案，两者共用同一份时区规则。放在 `core`
//! 而不是 `ui`，是为了让「领域逻辑不依赖界面」这条分层约定真正成立
//! （`core::data` 曾直接 `use crate::ui::…`）。

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 Unix 秒。取不到系统时间时退化为 0，而不是 panic。
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 时间戳所属的**本地**自然日。
pub fn local_date(timestamp: u64) -> NaiveDate {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|d| d.with_timezone(&Local).date_naive())
        .unwrap_or_else(|| Local::now().date_naive())
}

/// 本地时区下某一天的秒区间 `[起, 止)`，用于把「某天明细」变成一次索引区间查询。
/// 与 `local_date()` 共用同一套时区规则，因此区间内的记录与「按时间戳判定的本地日期」严格一致。
/// 只有本地零点不存在的时区（DST 会跳过零点的少数地区）才会返回 `None`，由调用方降级处理。
pub fn day_bounds(day: NaiveDate) -> Option<(u64, u64)> {
    Some((local_midnight(day)?, local_midnight(day.succ_opt()?)?))
}

fn local_midnight(day: NaiveDate) -> Option<u64> {
    day.and_hms_opt(0, 0, 0)?
        .and_local_timezone(Local)
        .earliest()
        .map(|dt| dt.timestamp() as u64)
}

pub fn format_clock(timestamp: u64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|d| d.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".into())
}

/// 带秒的时钟，用于提醒浮层里括号内的「具体时间」。
pub fn format_clock_secs(timestamp: u64) -> String {
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|d| d.with_timezone(&Local).format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".into())
}

/// 把秒数写成「1 天 2 小时 15 分 30 秒」：从最大非零单位一路展开到秒，
/// 保证文本每秒都会变化，同时不会出现「1 小时 59 秒」这种有歧义的省略写法。
pub fn format_span(seconds: u64) -> String {
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (hours, rest) = (rest / 3_600, rest % 3_600);
    let (minutes, secs) = (rest / 60, rest % 60);

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{} 天", days));
        parts.push(format!("{} 小时", hours));
        parts.push(format!("{} 分", minutes));
    } else if hours > 0 {
        parts.push(format!("{} 小时", hours));
        parts.push(format!("{} 分", minutes));
    } else if minutes > 0 {
        parts.push(format!("{} 分", minutes));
    }
    parts.push(format!("{} 秒", secs));
    parts.join(" ")
}

/// 相对日期前缀：当天为空串，之后是「昨天 」/「前天 」/「N 天前 」。
pub fn format_day_label(day: NaiveDate, today: NaiveDate) -> String {
    match today.signed_duration_since(day).num_days() {
        days if days <= 0 => String::new(),
        1 => "昨天 ".into(),
        2 => "前天 ".into(),
        days => format!("{} 天前 ", days),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// 检查单位边界及跨单位时不能省略中间单位。
    #[test]
    fn format_span_covers_boundaries_and_intermediate_units() {
        for (seconds, expected) in [
            (0, "0 秒"),
            (59, "59 秒"),
            (60, "1 分 0 秒"),
            (61, "1 分 1 秒"),
            (3600, "1 小时 0 分 0 秒"),
            (3659, "1 小时 0 分 59 秒"),
            (3661, "1 小时 1 分 1 秒"),
            (86_459, "1 天 0 小时 0 分 59 秒"),
            (176_400, "2 天 1 小时 0 分 0 秒"),
        ] {
            assert_eq!(format_span(seconds), expected);
        }
    }

    #[test]
    fn format_day_label_covers_relative_words_and_future() {
        let today = date(2026, 10, 5);
        assert_eq!(format_day_label(today, today), "");
        assert_eq!(format_day_label(date(2026, 10, 4), today), "昨天 ");
        assert_eq!(format_day_label(date(2026, 10, 3), today), "前天 ");
        assert_eq!(format_day_label(date(2026, 10, 2), today), "3 天前 ");
        // 未来日期（时钟回拨等）当作当天处理，不产生「-1 天前」这种文案。
        assert_eq!(format_day_label(date(2026, 10, 6), today), "");
    }

    /// `day_bounds` 是「某天明细」查询的区间来源，区间一旦与 `local_date` 不一致，
    /// 日历上那天的次数与点开后的明细就会对不上。这里钉住两者的互逆关系。
    #[test]
    fn day_bounds_agrees_with_local_date() {
        for day in [date(2026, 1, 1), date(2026, 6, 15), date(2026, 12, 31)] {
            let Some((start, end)) = day_bounds(day) else {
                continue; // 本地零点不存在的时区，调用方走降级分支
            };
            assert!(start < end, "{day} 的区间必须非空");
            assert_eq!(local_date(start), day, "区间起点必须落在 {day}");
            // 结束是次日零点：最后一秒仍属于当天。
            assert_eq!(local_date(end - 1), day, "区间终点前一刻仍属于 {day}");
            assert_eq!(local_date(end), day.succ_opt().unwrap());
        }
    }

    /// 相邻两天的区间必须首尾相接，否则跨天的那条记录会同时进两天或两天都不进。
    #[test]
    fn day_bounds_tile_without_gap_or_overlap() {
        let day = date(2026, 3, 8); // 含北美 DST 切换的日期，最容易被写错
        let (_, end) = day_bounds(day).unwrap();
        let (next_start, _) = day_bounds(day.succ_opt().unwrap()).unwrap();
        assert_eq!(end, next_start);
    }
}
