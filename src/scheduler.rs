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
                        match wait_for_resume(&cr) {
                            Some(next) => interval = next,
                            None => break,
                        }
                    }
                    Ok(AppCmd::Trigger) => {
                        if at.send(AppCmd::ShowOverlay).is_err() {
                            break;
                        }
                        match wait_for_resume(&cr) {
                            Some(next) => interval = next,
                            None => break,
                        }
                    }
                    Ok(AppCmd::ShowOverlay) => {}
                }
            }
        }
    });
    (ct, ar)
}

fn wait_for_resume(control_rx: &mpsc::Receiver<AppCmd>) -> Option<Duration> {
    loop {
        match control_rx.recv() {
            Ok(AppCmd::Reset(next)) => return Some(next),
            Ok(AppCmd::Stop) | Err(mpsc::RecvError) => return None,
            Ok(AppCmd::Trigger) | Ok(AppCmd::ShowOverlay) => {}
        }
    }
}
