use crate::config::Settings;
use std::{sync::mpsc, thread, time::Duration};
pub enum AppCmd {
    Reset(Duration),
    Stop,
    Trigger,
    ShowOverlay,
}
pub fn start_scheduler(settings: &Settings) -> (mpsc::Sender<AppCmd>, mpsc::Receiver<AppCmd>) {
    let (ct, cr) = mpsc::channel();
    let (at, ar) = mpsc::channel();
    thread::spawn({
        let initial = Duration::from_secs(settings.interval_secs);
        move || {
            let mut interval = initial;
            loop {
                match cr.recv_timeout(interval) {
                    Ok(AppCmd::Reset(next)) => interval = next,
                    Ok(AppCmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if at.send(AppCmd::ShowOverlay).is_err() {
                            break;
                        }
                    }
                    Ok(AppCmd::Trigger) => {
                        if at.send(AppCmd::ShowOverlay).is_err() {
                            break;
                        }
                    }
                    Ok(AppCmd::ShowOverlay) => {}
                }
            }
        }
    });
    (ct, ar)
}
