use crate::config::Settings;
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
pub enum AppCmd {
    Reset(Duration),
    Stop,
    Trigger,
    ShowOverlay(Duration),
}
pub fn start_scheduler(settings: &Settings) -> (mpsc::Sender<AppCmd>, mpsc::Receiver<AppCmd>) {
    let (ct, cr) = mpsc::channel();
    let (at, ar) = mpsc::channel();
    thread::spawn({
        let initial = Duration::from_secs(settings.interval_secs);
        move || {
            let mut interval = initial;
            let mut started_at = Instant::now();
            loop {
                match cr.recv_timeout(interval) {
                    Ok(AppCmd::Reset(next)) => {
                        interval = next;
                        started_at = Instant::now();
                    }
                    Ok(AppCmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if at.send(AppCmd::ShowOverlay(interval)).is_err() {
                            break;
                        }
                        started_at = Instant::now();
                    }
                    Ok(AppCmd::Trigger) => {
                        let remaining = interval.saturating_sub(started_at.elapsed());
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
