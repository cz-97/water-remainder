use crate::core::{config::MIN_INTERVAL, data::get_elapsed};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};

pub fn get_deadline(interval: Duration) -> SystemTime {
    SystemTime::now()
        + match get_elapsed() {
            Some(elapsed) => interval.saturating_sub(Duration::from_secs(elapsed)),
            None => Duration::default(),
        }
}

/// UI → 调度线程。
pub enum SchedulerCmd {
    Reschedule(RescheduleType),
    Stop,
    Trigger,
}

/// 调度线程 → UI。与 `SchedulerCmd` 分开成两个枚举，两个方向各自穷尽：
/// UI 侧不再需要用一个 catch-all 分支吞掉「只发给调度器的命令」。
pub enum SchedulerEvent {
    /// 到点或手动触发，`remaining` 是距下一次提醒的秒数。
    Remind { remaining: u64 },
}

pub enum RescheduleType {
    Drink,
    Wake,
    ChangeInterval(u64),
}

/// 启动调度线程。间隔由调用方传入（启动时已从设置里读过），
/// 线程内因此不再自己读一遍设置文件，两次读之间也不存在不一致窗口。
pub fn start_scheduler(
    interval_secs: u64,
) -> (mpsc::Sender<SchedulerCmd>, mpsc::Receiver<SchedulerEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    thread::spawn(move || {
        // 再兜一次底：设置文件可以手改，间隔为 0 会让下面的
        // `recv_timeout(0)` 立刻超时、deadline 又永远已过，退化成紧循环。
        let mut interval = Duration::from_secs(interval_secs.max(MIN_INTERVAL));
        let mut deadline = get_deadline(interval);
        loop {
            let remaining = deadline
                .duration_since(SystemTime::now())
                .unwrap_or_default();
            let wait = remaining.min(Duration::from_secs(1));
            match cmd_rx.recv_timeout(wait) {
                Ok(SchedulerCmd::Reschedule(reschedule_type)) => {
                    deadline = match reschedule_type {
                        RescheduleType::Drink => SystemTime::now() + interval,
                        RescheduleType::Wake => get_deadline(interval),
                        RescheduleType::ChangeInterval(interval_secs) => {
                            interval = Duration::from_secs(interval_secs.max(MIN_INTERVAL));
                            get_deadline(interval)
                        }
                    };
                }
                Ok(SchedulerCmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if deadline.duration_since(SystemTime::now()).is_err() {
                        deadline = SystemTime::now() + interval;
                        let event = SchedulerEvent::Remind {
                            remaining: interval.as_secs(),
                        };
                        if event_tx.send(event).is_err() {
                            break;
                        }
                    }
                }
                Ok(SchedulerCmd::Trigger) => {
                    let remaining = deadline
                        .duration_since(SystemTime::now())
                        .unwrap_or_default()
                        .as_secs();
                    if event_tx.send(SchedulerEvent::Remind { remaining }).is_err() {
                        break;
                    }
                }
            }
        }
    });
    (cmd_tx, event_rx)
}
