#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;
mod overlay;
mod platform;
mod scheduler;
mod tray;

use config::{Store, load_store, save_store};
use gpui::App;
use gpui_platform::application;
use overlay::open_overlay;
use scheduler::{AppCmd, start_scheduler};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use tray::setup_tray;
use tray_icon::menu::MenuEvent;

fn main() {
    if !platform::ensure_single_instance() {
        return;
    }
    application()
        .with_quit_mode(gpui::QuitMode::Explicit)
        .run(|cx: &mut App| {
            let store = Arc::new(Mutex::new(load_store()));
            let settings = store.lock().unwrap().settings.clone();
            platform::enable_system_menu_theme();
            platform::set_autostart(settings.autostart);
            let (tray, show, quit, intervals, startup, startup_remind) = setup_tray(&settings);
            std::mem::forget(tray);
            let (scheduler_tx, alarm_rx) = start_scheduler(&settings);
            if settings.startup_remind {
                let _ = scheduler_tx.send(AppCmd::Trigger);
            }
            let show_id = show.id().clone();
            let quit_id = quit.id().clone();
            let startup_id = startup.id().clone();
            let startup_remind_id = startup_remind.id().clone();
            let interval_ids: Vec<_> = intervals
                .iter()
                .map(|(seconds, item)| (*seconds, item.id().clone(), item.clone()))
                .collect();
            let menu_rx = MenuEvent::receiver();
            let shared = store.clone();
            cx.spawn(async move |cx| {
                loop {
                    while let Ok(event) = menu_rx.try_recv() {
                        if event.id == show_id {
                            let _ = scheduler_tx.send(AppCmd::Trigger);
                        } else if event.id == quit_id {
                            let _ = scheduler_tx.send(AppCmd::Stop);
                            cx.update(|cx| cx.quit());
                            return;
                        } else if event.id == startup_id {
                            if let Ok(mut store) = shared.lock() {
                                store.settings.autostart = !store.settings.autostart;
                                platform::set_autostart(store.settings.autostart);
                                save_store(&store);
                            }
                        } else if event.id == startup_remind_id {
                            if let Ok(mut store) = shared.lock() {
                                store.settings.startup_remind = !store.settings.startup_remind;
                                save_store(&store);
                            }
                        } else if let Some((seconds, _, _)) =
                            interval_ids.iter().find(|(_, id, _)| *id == event.id)
                        {
                            for (_, _, item) in &interval_ids {
                                item.set_checked(event.id == item.id());
                            }
                            if let Ok(mut store) = shared.lock() {
                                store.settings.interval_secs = *seconds;
                                save_store(&store);
                                let _ =
                                    scheduler_tx.send(AppCmd::Reset(Duration::from_secs(*seconds)));
                            }
                        }
                    }
                    while let Ok(AppCmd::ShowOverlay) = alarm_rx.try_recv() {
                        show_overlay(cx, scheduler_tx.clone(), shared.clone());
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }
            })
            .detach();
        });
}

fn show_overlay(
    cx: &mut gpui::AsyncApp,
    scheduler: mpsc::Sender<AppCmd>,
    store: Arc<Mutex<Store>>,
) {
    let _ = cx.update(|cx| open_overlay(cx, scheduler, store));
}
