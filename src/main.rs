#![cfg_attr(windows, windows_subsystem = "windows")]

mod core;
mod platform;
mod ui;

use core::config::{load_store, settings_snapshot};
use core::scheduler::{RescheduleType, SchedulerCmd, SchedulerEvent, start_scheduler};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_util::StreamExt;
use gpui_kit::App;
use gpui_kit::platform::application;
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
        .with_quit_mode(gpui_kit::QuitMode::Explicit)
        .run(|cx: &mut App| {
            // gpui-kit 的契约：开窗前初始化已启用的层。本项目 `default-features = false`，
            // 只启用 gpui 层，所以这里只登记了 gpui-base 的主题与控件全局态，界面自绘不读它。
            gpui_kit::init(cx);
            let store = Arc::new(Mutex::new(load_store()));
            let settings = settings_snapshot(&store);
            // 把用户选的主题写进全局配色，早于任何窗口创建 —— 否则首帧会用默认的
            // 「跟随系统」渲染出一瞬，再被用户设置纠正（肉眼可见的闪一下）。
            ui::palette::set_preference(settings.theme);
            platform::enable_system_menu_theme();
            // 启动时把注册表对齐到设置里的 `autostart`。失败只意味着这次没对上，
            // 不影响程序使用，因此提示但不中断：用户至少能从弹出框知道原因。
            if let Err(error) = platform::set_autostart(settings.autostart) {
                platform::warn(&format!("{error}\n\n开机启动设置未能生效。"));
            }
            // 托盘建不起来就没有常驻入口、也没有退出方式，因此这是致命错误：提示用户
            // 后直接退出，而不是让 `expect` 在 release（`panic = "abort"`）下无声消失。
            let (tray, show, quit) = match setup_tray() {
                Ok(tray) => tray,
                Err(error) => platform::fatal_startup_error(&format!(
                    "无法创建托盘图标，程序无法继续运行。\n\n{error}"
                )),
            };
            std::mem::forget(tray);
            let (scheduler_tx, alarm_rx) = start_scheduler(settings.interval_secs);
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
                                cx.update(|cx| open_main_window(cx, records, scheduler_tx.clone()));
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
    cx.update(|cx| open_reminder_window(cx, scheduler, remaining));
}
