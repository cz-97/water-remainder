use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// 托盘右键菜单里需要被主线程按 id 区分的菜单项。
///
/// 单独抽出来而不是把 `MenuItem` 一路返回成元组：菜单项增删时（本次加了
/// 「设置」）调用方要改的是同一个类型，不存在「忘了同步第 N 个元组位置」
/// 的机会。
pub struct TrayMenuItems {
    pub show: MenuItem,
    pub settings: MenuItem,
    pub quit: MenuItem,
}

/// 建立托盘图标与右键菜单。
///
/// 返回 `Result` 而不是在内部 `expect`：托盘是程序唯一的常驻入口，也是唯一的
/// 退出方式。创建失败时进程若直接消失（release 下 `panic = "abort"`），用户既看
/// 不到界面，也拿不到任何线索。失败原因交给调用方去弹原生提示框，
/// 见 `platform::fatal_startup_error`。
///
/// 只有「托盘图标本身建不起来」才算失败。往菜单里追加条目的失败**不影响程序可用**：
/// 图标照常出现，左键照常开会主窗，只是右键菜单可能少一两项。为这种局部降级
/// 而拒绝启动，是把一个可恢复的问题放大成不可用，因此这里仍然沿用 `.ok()` 忽略。
pub fn setup_tray() -> Result<(TrayIcon, TrayMenuItems), String> {
    let show = MenuItem::new("立即提醒", true, None);
    let settings = MenuItem::new("设置", true, None);
    let quit = MenuItem::new("退出", true, None);
    let menu = Menu::new();
    menu.append(&show).ok();
    menu.append(&settings).ok();
    menu.append(&PredefinedMenuItem::separator()).ok();
    menu.append(&quit).ok();
    let tray = TrayIconBuilder::new()
        .with_tooltip("喝水提醒")
        .with_icon(icon()?)
        .with_menu_on_left_click(false)
        .with_menu(Box::new(menu))
        .build()
        .map_err(|error| format!("创建托盘图标失败：{error}"))?;
    Ok((
        tray,
        TrayMenuItems {
            show,
            settings,
            quit,
        },
    ))
}

fn icon() -> Result<tray_icon::Icon, String> {
    #[cfg(windows)]
    {
        // 图标来自 `water-remainder.rc` 里嵌入的资源（序号 1）。取不到说明构建
        // 产物不完整，但同样不能静默退出 —— 交给调用方提示用户。
        tray_icon::Icon::from_resource(1, None)
            .map_err(|error| format!("读取嵌入图标资源失败：{error}"))
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
        tray_icon::Icon::from_rgba(rgba, n as u32, n as u32)
            .map_err(|error| format!("生成托盘图标失败：{error}"))
    }
}
