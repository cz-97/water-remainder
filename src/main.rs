#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;
mod main_window;
mod platform;
mod reminder_window;
mod scheduler;
mod settings_window;
mod tray;
mod ui;

use config::{Store, load_store};
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
use ui::now;

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
            let (tray, show, quit) = setup_tray();
            std::mem::forget(tray);
            let first_delay = first_reminder_delay(&store, settings.interval_secs);
            let remind_on_start = first_delay.is_zero();
            let initial_delay = if remind_on_start {
                Duration::from_secs(settings.interval_secs)
            } else {
                first_delay
            };
            let (scheduler_tx, alarm_rx) = start_scheduler(&settings, initial_delay);
            if remind_on_start {
                let _ = scheduler_tx.send(AppCmd::Trigger);
            }
            let show_id = show.id().clone();
            let quit_id = quit.id().clone();
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
                                let _ = cx.update(|cx| {
                                    open_main_window(cx, records, scheduler_tx.clone())
                                });
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
                            }
                        }
                        AppEvent::Alarm(AppCmd::ShowOverlay(remaining)) => {
                            show_reminder(cx, scheduler_tx.clone(), shared.clone(), remaining);
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
    remaining: Duration,
) {
    let _ = cx.update(|cx| open_reminder_window(cx, scheduler, store, remaining));
}

fn first_reminder_delay(store: &Arc<Mutex<Store>>, interval_secs: u64) -> Duration {
    let Ok(store) = store.lock() else {
        return Duration::from_secs(interval_secs);
    };
    let Some(&last_drink) = store.drinks.iter().max() else {
        return Duration::from_secs(interval_secs);
    };
    let elapsed = now().saturating_sub(last_drink);
    Duration::from_secs(interval_secs.saturating_sub(elapsed))
}
