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
            let mut deadline = Instant::now() + initial;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match cr.recv_timeout(remaining) {
                    Ok(AppCmd::Reset(next)) => {
                        interval = next;
                        deadline = Instant::now() + next;
                    }
                    Ok(AppCmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        deadline = Instant::now() + interval;
                        if at.send(AppCmd::ShowOverlay(interval)).is_err() {
                            break;
                        }
                    }
                    Ok(AppCmd::Trigger) => {
                        let remaining = deadline.saturating_duration_since(Instant::now());
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
