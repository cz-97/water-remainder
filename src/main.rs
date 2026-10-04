#![cfg_attr(windows, windows_subsystem = "windows")]

mod core;
mod platform;
mod ui;

use core::config::load_store;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_util::StreamExt;
use gpui_kit::App;
use gpui_kit::platform::application;
use core::scheduler::{RescheduleType, SchedulerCmd, SchedulerEvent, start_scheduler};
use platform::tray::setup_tray;
use std::sync::{Arc, Mutex, mpsc};
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
use ui::main_window::open_main_window;
use ui::reminder_window::open_reminder_window;

enum AppEvent {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
    Alarm(SchedulerEvent),
}

fn send_event(tx: &UnboundedSender<AppEvent>, event: AppEvent) {
    let _ = tx.unbounded_send(event);
}

fn main() {
    if !platform::ensure_single_instance() {
        return;
    }
    application()
        .with_assets(gpui_kit::assets::Assets)
        .with_quit_mode(gpui_kit::QuitMode::Explicit)
        .run(|cx: &mut App| {
            // gpui-kit 的契约：开窗前初始化已启用的层。本项目 `default-features = false`，
            // 只启用 gpui 层，所以这里只登记了 gpui-base 的主题与控件全局态，界面自绘不读它。
            gpui_kit::init(cx);
            // 组件库默认浅色主题；本项目界面为深色，统一切到暗色。
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            let store = Arc::new(Mutex::new(load_store()));
            let settings = store.lock().unwrap().settings.clone();
            platform::enable_system_menu_theme();
            platform::set_autostart(settings.autostart);
            let (tray, show, quit) = setup_tray();
            std::mem::forget(tray);
            let (scheduler_tx, alarm_rx) = start_scheduler();
            // 睡眠唤醒后：与启动一致，根据最近一次喝水记录重新计算第一次提醒。
            // `Subscription` 是 RAII 守卫，drop 即注销回调。而 `run` 的启动闭包在
            // 消息循环开始之前就返回了（`gpui-pre-windows/src/platform.rs:508`：先调用
            // `on_finish_launching()`，之后才 `GetMessageW`），绑定在闭包里的守卫会被
            // 立刻丢弃 —— 必须 `detach()`，否则唤醒回调永远不会被调用。
            // 上面 `std::mem::forget(tray)` 处理的是同一个「闭包提前返回」问题。
            let wake_tx = scheduler_tx.clone();
            cx.on_system_wake(move |_| {
                let _ = wake_tx.send(SchedulerCmd::Reschedule(RescheduleType::Wake));
            })
            .detach();
            let show_id = show.id().clone();
            let quit_id = quit.id().clone();
            let shared_store = store.clone();
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
                                let records = shared_store.clone();
                                let _ = cx.update(|cx| {
                                    open_main_window(cx, records, scheduler_tx.clone())
                                });
                            }
                        }
                        AppEvent::Menu(event) => {
                            if event.id == show_id {
                                let _ = scheduler_tx.send(SchedulerCmd::Trigger);
                            } else if event.id == quit_id {
                                let _ = scheduler_tx.send(SchedulerCmd::Stop);
                                cx.update(|cx| {
                                    ui::main_window::save_main_window_state(cx);
                                    cx.quit();
                                });
                                return;
                            }
                        }
                        AppEvent::Alarm(SchedulerEvent::Remind { remaining }) => {
                            show_reminder(cx, scheduler_tx.clone(), remaining);
                        }
                    }
                }
            })
            .detach();
        });
}

fn show_reminder(
    cx: &mut gpui_kit::AsyncApp,
    scheduler: mpsc::Sender<SchedulerCmd>,
    remaining: u64,
) {
    let _ = cx.update(|cx| open_reminder_window(cx, scheduler, remaining));
}
