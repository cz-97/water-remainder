use crate::config::load_store;
use crate::data::get_last_time;
use std::{
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// 与启动策略一致：根据“最近一次喝水（或没有记录）”计算下一次提醒的等待时间。
/// - 没有记录：完整提醒间隔；
/// - 已过 `>=` 间隔：立即（0），之后按完整间隔；
/// - 已过 `<` 间隔：“间隔 − 已过”。
pub fn get_deadline() -> (Duration, SystemTime) {
    let interval_secs = load_store().settings.interval_secs;
    let interval = Duration::from_secs(interval_secs);
    let now = SystemTime::now();
    if let Some(last_time) = get_last_time() {
        return (interval, now + Duration::from_secs(interval_secs.saturating_sub(
            now.duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default()
                .saturating_sub(last_time),
        )));
    }
    (interval, now + interval)
}

pub enum AppCmd {
    /// 启动 / 睡眠唤醒 / 修改提醒间隔时/ 用户喝水后重排到 `now + delay`。
    Reschedule,
    /// 重置提醒间隔为完整间隔。
    Reset,
    Stop,
    Trigger,
    ShowOverlay(Duration),
}

pub fn start_scheduler() -> (mpsc::Sender<AppCmd>, mpsc::Receiver<AppCmd>) {
    let (ct, cr) = mpsc::channel();
    let (at, ar) = mpsc::channel();
    thread::spawn({
        move || {
            let (mut interval, mut deadline) = get_deadline();
            loop {
                let remaining = deadline
                    .duration_since(SystemTime::now())
                    .unwrap_or_default();
                let wait = remaining.min(Duration::from_secs(1));
                match cr.recv_timeout(wait) {
                    Ok(AppCmd::Reschedule) => {
                        (interval, deadline) = get_deadline();
                    }
                    Ok(AppCmd::Reset) => {
                        deadline = SystemTime::now() + interval;
                    }
                    Ok(AppCmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if deadline.duration_since(SystemTime::now()).is_err() {
                            deadline = SystemTime::now() + interval;
                            if at.send(AppCmd::ShowOverlay(interval)).is_err() {
                                break;
                            }
                        }
                    }
                    Ok(AppCmd::Trigger) => {
                        let remaining = deadline
                            .duration_since(SystemTime::now())
                            .unwrap_or_default();
                        if at.send(AppCmd::ShowOverlay(remaining)).is_err() {
                            break;
                        }
                    }
                    Ok(AppCmd::ShowOverlay(_)) => {}
                }
            }
        }
    });
    (ct, ar)
}
