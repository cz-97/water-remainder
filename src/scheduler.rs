use crate::{config::load_store, data::get_elapsed};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};

pub fn get_deadline(interval: Duration) -> SystemTime {
    SystemTime::now()
        + interval.saturating_sub(Duration::from_secs(get_elapsed().unwrap_or_default()))
}

pub enum AppCmd {
    Reschedule(RescheduleType),
    Stop,
    Trigger,
    ShowOverlay(u64),
}
pub enum RescheduleType {
    Drink,
    Wake,
    ChangeInterval(u64),
}

pub fn start_scheduler() -> (mpsc::Sender<AppCmd>, mpsc::Receiver<AppCmd>) {
    let (ct, cr) = mpsc::channel();
    let (at, ar) = mpsc::channel();
    thread::spawn({
        move || {
            let mut interval = Duration::from_secs(load_store().settings.interval_secs);
            let mut deadline = get_deadline(interval);
            loop {
                let remaining = deadline
                    .duration_since(SystemTime::now())
                    .unwrap_or_default();
                let wait = remaining.min(Duration::from_secs(1));
                match cr.recv_timeout(wait) {
                    Ok(AppCmd::Reschedule(reschedule_type)) => {
                        deadline = match reschedule_type {
                            RescheduleType::Drink => SystemTime::now() + interval,
                            RescheduleType::Wake => get_deadline(interval),
                            RescheduleType::ChangeInterval(interval_secs) => {
                                interval = Duration::from_secs(interval_secs);
                                get_deadline(interval)
                            }
                        };
                    }
                    Ok(AppCmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if deadline.duration_since(SystemTime::now()).is_err() {
                            deadline = SystemTime::now() + interval;
                            if at.send(AppCmd::ShowOverlay(interval.as_secs())).is_err() {
                                break;
                            }
                        }
                    }
                    Ok(AppCmd::Trigger) => {
                        let remaining = deadline
                            .duration_since(SystemTime::now())
                            .unwrap_or_default()
                            .as_secs();
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
