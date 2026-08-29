#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;
mod main_window;
mod platform;
mod reminder_window;
mod scheduler;
mod tray;
mod ui;

use config::{Store, load_store, save_store};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_util::StreamExt;
use gpui::App;
use gpui_platform::application;
use main_window::open_main_window;
use reminder_window::open_reminder_window;
use scheduler::{AppCmd, start_scheduler};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use tray::setup_tray;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

enum AppEvent {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
    Alarm(AppCmd),
}

fn send_event(tx: &UnboundedSender<AppEvent>, event: AppEvent) {
    let _ = tx.unbounded_send(event);
}

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
            let shared = store.clone();
            let (event_tx, event_rx) = unbounded();
            let tray_tx = event_tx.clone();
            TrayIconEvent::set_event_handler(Some(move |event| {
                send_event(&tray_tx, AppEvent::Tray(event));
            }));
            let menu_tx = event_tx.clone();
            MenuEvent::set_event_handler(Some(move |event| {
                send_event(&menu_tx, AppEvent::Menu(event));
            }));
            let alarm_tx = event_tx.clone();
            std::thread::spawn(move || {
                while let Ok(event) = alarm_rx.recv() {
                    if alarm_tx.unbounded_send(AppEvent::Alarm(event)).is_err() {
                        break;
                    }
                }
            });
            cx.spawn(async move |cx| {
                let mut event_rx: UnboundedReceiver<AppEvent> = event_rx;
                while let Some(event) = event_rx.next().await {
                    match event {
                        AppEvent::Tray(event) => {
                            if let TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Up,
                                ..
                            } = event
                            {
                                let records = shared.clone();
                                let _ = cx.update(|cx| open_main_window(cx, records));
                            }
                        }
                        AppEvent::Menu(event) => {
                            if event.id == show_id {
                                let _ = scheduler_tx.send(AppCmd::Trigger);
                            } else if event.id == quit_id {
                                let _ = scheduler_tx.send(AppCmd::Stop);
                                cx.update(|cx| {
                                    main_window::save_main_window_state(cx);
                                    cx.quit();
                                });
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
                                    let _ = scheduler_tx
                                        .send(AppCmd::Reset(Duration::from_secs(*seconds)));
                                }
                            }
                        }
                        AppEvent::Alarm(AppCmd::ShowOverlay) => {
                            show_reminder(cx, scheduler_tx.clone(), shared.clone());
                        }
                        AppEvent::Alarm(_) => {}
                    }
                }
            })
            .detach();
        });
}

fn show_reminder(
    cx: &mut gpui::AsyncApp,
    scheduler: mpsc::Sender<AppCmd>,
    store: Arc<Mutex<Store>>,
) {
    let _ = cx.update(|cx| open_reminder_window(cx, scheduler, store));
}
