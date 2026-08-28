use crate::config::{INTERVALS, Settings};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};
pub fn setup_tray(
    s: &Settings,
) -> (
    TrayIcon,
    MenuItem,
    MenuItem,
    Vec<(u64, CheckMenuItem)>,
    CheckMenuItem,
    CheckMenuItem,
) {
    let show = MenuItem::new("立即提醒", true, None);
    let quit = MenuItem::new("退出", true, None);
    let startup = CheckMenuItem::new("开机启动", true, s.autostart, None);
    let startup_remind = CheckMenuItem::new("启动时立即提醒", true, s.startup_remind, None);
    let sub = Submenu::new("提醒间隔", true);
    let mut ints = Vec::new();
    for &(secs, label) in INTERVALS {
        let item = CheckMenuItem::new(label, true, s.interval_secs == secs, None);
        sub.append(&item).ok();
        ints.push((secs, item));
    }
    let menu = Menu::new();
    menu.append(&show).ok();
    menu.append(&sub).ok();
    menu.append(&startup).ok();
    menu.append(&startup_remind).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&quit).ok();
    let tray = TrayIconBuilder::new()
        .with_tooltip("喝水提醒")
        .with_icon(icon())
        .with_menu(Box::new(menu))
        .build()
        .expect("create tray");
    (tray, show, quit, ints, startup, startup_remind)
}
fn icon() -> tray_icon::Icon {
    #[cfg(windows)]
    {
        return tray_icon::Icon::from_resource(1, None).expect("embedded water.ico");
    }
    #[cfg(not(windows))]
    {
        let n = 32;
        let mut rgba = vec![0u8; n * n * 4];
        for p in rgba.chunks_exact_mut(4) {
            p[0] = 0x3b;
            p[1] = 0x82;
            p[2] = 0xf6;
            p[3] = 0xff;
        }
        tray_icon::Icon::from_rgba(rgba, n as u32, n as u32).expect("icon")
    }
}
