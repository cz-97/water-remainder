use crate::config::Settings;
use std::{
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};
pub enum AppCmd {
    Reset(Duration),
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
