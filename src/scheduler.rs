use crate::config::Settings;
use std::{
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// 与启动策略一致：根据“最近一次喝水（或没有记录）”计算下一次提醒的等待时间。
/// - 没有记录：完整提醒间隔；
/// - 已过 `>=` 间隔：立即（0），之后按完整间隔；
/// - 已过 `<` 间隔：“间隔 − 已过”。
pub fn delay_from_last_drink(last_drink: Option<u64>, interval_secs: u64) -> Duration {
    let Some(last) = last_drink else {
        return Duration::from_secs(interval_secs);
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    Duration::from_secs(interval_secs.saturating_sub(now.saturating_sub(last)))
}

pub enum AppCmd {
    /// 用户喝水后重排：下一次提醒 = 喝水时刻 + 完整间隔（等价于 delay = 完整间隔）。
    Reset(Duration),
    /// 启动 / 睡眠唤醒 / 修改提醒间隔时重排到 `now + delay`。
    Reschedule { interval: Duration, delay: Duration },
    Stop,
    Trigger,
    ShowOverlay(Duration),
}

pub fn start_scheduler(
    settings: &Settings,
    initial_delay: Duration,
) -> (mpsc::Sender<AppCmd>, mpsc::Receiver<AppCmd>) {
    let (ct, cr) = mpsc::channel();
    let (at, ar) = mpsc::channel();
    thread::spawn({
        let initial = Duration::from_secs(settings.interval_secs);
        move || {
            let mut interval = initial;
            let mut deadline = SystemTime::now() + initial_delay;

            loop {
                let remaining = deadline
                    .duration_since(SystemTime::now())
                    .unwrap_or_default();
                let wait = remaining.min(Duration::from_secs(1));
                match cr.recv_timeout(wait) {
                    Ok(AppCmd::Reset(next)) => {
                        interval = next;
                        deadline = SystemTime::now() + next;
                    }
                    Ok(AppCmd::Reschedule {
                        interval: next,
                        delay,
                    }) => {
                        interval = next;
                        deadline = SystemTime::now() + delay;
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
