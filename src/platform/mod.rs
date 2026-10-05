pub mod tray;

#[cfg(windows)]
pub fn enable_system_menu_theme() {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
    use windows::core::{PCSTR, s};

    // SetPreferredAppMode is an undocumented uxtheme API used by Windows'
    // own menu implementation. `1` is the AllowDark value; with this mode
    // enabled, muda's `MenuTheme::Auto` can follow the Windows app theme.
    const SET_PREFERRED_APP_MODE: usize = 135;
    type SetPreferredAppMode = unsafe extern "system" fn(usize) -> usize;

    unsafe {
        let Ok(uxtheme) = LoadLibraryA(s!("uxtheme.dll")) else {
            return;
        };
        let Some(proc) = GetProcAddress(
            uxtheme,
            PCSTR::from_raw(SET_PREFERRED_APP_MODE as *const u8),
        ) else {
            return;
        };
        let set_preferred_app_mode: SetPreferredAppMode = std::mem::transmute(proc);
        set_preferred_app_mode(1);
    }
}

#[cfg(not(windows))]
pub fn enable_system_menu_theme() {}

/// 非致命的启动警告：把原因告诉用户，然后继续运行。
///
/// 与 [`fatal_startup_error`] 的区别只在于「还能不能继续用」：托盘建不起来就彻底
/// 没有入口，必须退出；而注册表自启没写成功只是这一项没生效，程序照常能用，
/// 不该因此拒绝启动。同样走原生 `MessageBoxW` —— 这里往往还没有任何 GPUI 窗口。
#[cfg(windows)]
pub fn warn(message: &str) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    };
    use windows::core::HSTRING;

    let text = HSTRING::from(message);
    let caption = HSTRING::from("喝水提醒");
    unsafe {
        MessageBoxW(
            Some(HWND::default()),
            &text,
            &caption,
            MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
        );
    }
}

#[cfg(not(windows))]
pub fn warn(message: &str) {
    eprintln!("喝水提醒：{message}");
}

/// 启动期的致命错误：弹一个原生提示框把原因告诉用户，然后结束进程。
///
/// 走原生 `MessageBoxW` 而不是 GPUI 窗口，是因为这一步可能失败得比任何窗口都早
/// （托盘建不起来时，程序既没有界面，也没有退出入口）。返回 `!`，调用方因此不必
/// 再考虑「提示完还继续往下跑」的分支 —— 那正是原文里 `expect` 的问题：release 下
/// `panic = "abort"`，进程连同托盘图标一起无声消失，用户拿不到任何线索。
#[cfg(windows)]
pub fn fatal_startup_error(message: &str) -> ! {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    };
    use windows::core::HSTRING;

    let text = HSTRING::from(message);
    let caption = HSTRING::from("喝水提醒");
    unsafe {
        MessageBoxW(
            Some(HWND::default()),
            &text,
            &caption,
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
    std::process::exit(1);
}

#[cfg(not(windows))]
pub fn fatal_startup_error(message: &str) -> ! {
    eprintln!("喝水提醒启动失败：{message}");
    std::process::exit(1);
}

#[cfg(windows)]
/// 增删注册表 Run 键。返回失败原因，交给调用方决定怎么提示。
///
/// 不能吞掉错误：用户勾了「开机启动」而注册表没写进去，界面上的开关却是开着的，
/// 下次开机不会启动 —— 这种「显示成功、实际没做」正是最难排查的一类问题。
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegDeleteValueW, RegSetValueExW,
    };
    use windows::core::w;

    let mut key = HKEY::default();
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            Some(0),
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if result.0 != 0 {
        return Err(format!("无法打开开机启动注册表项（错误码 {}）", result.0));
    }

    let outcome = unsafe {
        if enabled {
            match std::env::current_exe() {
                Ok(exe) => {
                    // 路径必须带引号：Run 键的值会被 Windows 直接当命令行解析，装在含
                    // 空格的目录（如 `C:\Program Files\...`）时会被按空格切开，
                    // 开机启动于是静默失效。
                    let mut value: Vec<u16> =
                        format!("\"{}\"", exe.display()).encode_utf16().collect();
                    value.push(0);
                    let status = RegSetValueExW(
                        key,
                        w!("WaterRemainder"),
                        Some(0),
                        REG_SZ,
                        Some(std::slice::from_raw_parts(
                            value.as_ptr() as *const u8,
                            value.len() * 2,
                        )),
                    );
                    if status.0 == 0 {
                        Ok(())
                    } else {
                        Err(format!("写入开机启动项失败（错误码 {}）", status.0))
                    }
                }
                Err(error) => Err(format!("无法取得程序路径：{error}")),
            }
        } else {
            // 值本来就不存在时 RegDeleteValueW 会返回 ERROR_FILE_NOT_FOUND，
            // 那正是我们想要的结果，不算失败。
            let status = RegDeleteValueW(key, w!("WaterRemainder"));
            if status.0 == 0 || status.0 == ERROR_FILE_NOT_FOUND.0 {
                Ok(())
            } else {
                Err(format!("移除开机启动项失败（错误码 {}）", status.0))
            }
        }
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    outcome
}
#[cfg(not(windows))]
pub fn set_autostart(_: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub fn ensure_single_instance() -> bool {
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::core::w;
    unsafe {
        match CreateMutexW(None, true, w!("Local\\WaterRemainder.SingleInstance")) {
            // 句柄故意不关闭：进程存活期间持有互斥体，退出时由系统回收。
            // `HANDLE` 自身没有 Drop（RAII 由 `Owned<HANDLE>` 承担），
            // 因此这里直接丢弃绑定不会提前释放互斥体。
            Ok(_owner) => GetLastError() != ERROR_ALREADY_EXISTS,
            Err(_) => true,
        }
    }
}

#[cfg(not(windows))]
pub fn ensure_single_instance() -> bool {
    true
}

/// 从 GPUI 窗口取出原生 HWND。句柄提取与 `unsafe` 只在这一层出现。
#[cfg(windows)]
fn hwnd(window: &gpui_kit::Window) -> Option<windows::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // `Window` 自带一个同名的固有方法（返回 `AnyWindowHandle`），必须用完全限定
    // 语法才能拿到 raw-window-handle 的实现。
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(value) => {
            Some(windows::Win32::Foundation::HWND(value.hwnd.get() as *mut _))
        }
        _ => None,
    }
}

/// 设置 DWM 圆角偏好。主窗与浮层只差圆角常量，共用这一处调用。
///
/// 不再去除系统描边：去掉后窗口失去标准边框/阴影，圆角观感会与其它应用不一致；
/// 保留系统描边可让主窗与 Windows 上的普通窗口保持一致。
#[cfg(windows)]
fn set_window_chrome(
    hwnd: windows::Win32::Foundation::HWND,
    corner: windows::Win32::Graphics::Dwm::DWM_WINDOW_CORNER_PREFERENCE,
) {
    use windows::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute};

    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        );
    }
}

/// 主窗口：启用 DWM 圆角。
#[cfg(windows)]
pub fn style_main_window(window: &gpui_kit::Window) {
    use windows::Win32::Graphics::Dwm::DWMWCP_ROUND;

    if let Some(hwnd) = hwnd(window) {
        set_window_chrome(hwnd, DWMWCP_ROUND);
    }
}

#[cfg(not(windows))]
pub fn style_main_window(_: &gpui_kit::Window) {}

/// 提醒浮层：铺满整屏，真正无边框。
///
/// 两层问题要一起解决：
/// 1. **客户区要对齐显示器矩形** —— 只信 `display.bounds()` 不够，gpui 把逻辑像素
///    经 `to_device_pixels` 换回物理像素时，`1706.6666 × 1066.6666` 这类值会因
///    浮点截断少几个像素（实测正好 5px），底部就露出一条没被遮罩覆盖的窄带。
/// 2. **关掉 DWM 的非客户区渲染** —— 不关的话，Windows 给每个顶层窗口画的那圈
///    1px 描边 + 阴影会留在顶部，看起来就像一条白边（“像视频 / 游戏 / 浏览器
///    那样的全屏”正是关了这两样：无边框、无阴影）。
///
/// 客户区对齐用**实测差值**：分别拿客户区在屏幕上的四边与显示器矩形的四边，
/// 按差值调整位置与大小，使客户区恰好等于显示器矩形。全程物理像素，不经
/// 逻辑像素换算（5px 正是往返换算截断造成的）。
///
/// 这个函数是幂等的：已对齐时四个差值全为 0，直接返回。因此可以在窗口
/// 每次移动 / 改变尺寸时重跑（见 `refit_reminder_window`），不会因为
/// `SetWindowPos` 触发 `WM_MOVE` 而递归。
#[cfg(windows)]
pub fn style_reminder_window(window: &gpui_kit::Window) {
    use windows::Win32::Graphics::Dwm::{
        DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY,
        DWMWCP_DONOTROUND, DwmSetWindowAttribute,
    };

    let Some(hwnd) = hwnd(window) else {
        return;
    };
    // 圆角要先关掉：铺满整屏时圆角会露出下层桌面。
    set_window_chrome(hwnd, DWMWCP_DONOTROUND);

    unsafe {
        // 关掉 DWM 的非客户区渲染：去掉窗口四周的阴影与描边。
        let policy = DWMNCRP_DISABLED;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &policy as *const _ as *const _,
            std::mem::size_of_val(&policy) as u32,
        );
        // 再把那条 1px 描边颜色设成「无」：不设的话它默认是一条浅色线。
        let border_color = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border_color as *const _ as *const _,
            std::mem::size_of_val(&border_color) as u32,
        );
    }

    fit_client_to_monitor(hwnd);
}

#[cfg(not(windows))]
pub fn style_reminder_window(_: &gpui_kit::Window) {}

/// 浮层被移动 / 改变尺寸后重新对齐。
///
/// 必要性：浮层开着的时候用户可能改分辨率、拔掉或接上显示器。此时客户区
/// 与新显示器不再一致，缝隙会重新出现。`bounds` 观察者会反复回调，因此本函数
/// 必须幂等 —— 它确实是（见 `fit_client_to_monitor`）。
#[cfg(windows)]
pub fn refit_reminder_window(window: &gpui_kit::Window) {
    if let Some(hwnd) = hwnd(window) {
        fit_client_to_monitor(hwnd);
    }
}

#[cfg(not(windows))]
pub fn refit_reminder_window(_: &gpui_kit::Window) {}

/// 把**客户区**（真正被绘制的那块）拉到与显示器矩形重合。
///
/// 全程物理像素，不经逻辑像素换算 —— 5px 那个缝隙正是往返换算截断造成的。
/// 已经对齐时直接返回，所以可以随便重复调用。
#[cfg(windows)]
fn fit_client_to_monitor(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetWindowRect, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER, SetWindowPos,
    };

    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut monitor_info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // 取不到显示器矩形就不动：宁可保持原样，也不要猜一个位置把窗口甩出屏幕。
        if !GetMonitorInfoW(monitor, &mut monitor_info).as_bool() {
            return;
        }
        let screen = monitor_info.rcMonitor;

        let mut window_rect = RECT::default();
        let mut client_rect = RECT::default();
        if GetWindowRect(hwnd, &mut window_rect).is_err()
            || GetClientRect(hwnd, &mut client_rect).is_err()
        {
            return;
        }
        // `GetClientRect` 给的是客户区内的坐标，要靠 `ClientToScreen` 才知道
        // 它在屏幕上的实际位置。
        let mut client_top_left = POINT {
            x: client_rect.left,
            y: client_rect.top,
        };
        let mut client_bottom_right = POINT {
            x: client_rect.right,
            y: client_rect.bottom,
        };
        if !ClientToScreen(hwnd, &mut client_top_left).as_bool()
            || !ClientToScreen(hwnd, &mut client_bottom_right).as_bool()
        {
            return;
        }

        // 每条边各自要移动多少（向右 / 向下为正）。
        let move_x = screen.left - client_top_left.x;
        let move_y = screen.top - client_top_left.y;
        // 尺寸要补上移动带来的缺口：先把左边缘挪到位，右边缘就跟着变，
        // 所以宽高只补「移动之后仍然差的那部分」。
        let width_gap = (screen.right - client_bottom_right.x) - move_x;
        let height_gap = (screen.bottom - client_bottom_right.y) - move_y;

        if move_x == 0 && move_y == 0 && width_gap == 0 && height_gap == 0 {
            return; // 已对齐：这也是递归的终止条件
        }

        let _ = SetWindowPos(
            hwnd,
            None,
            window_rect.left + move_x,
            window_rect.top + move_y,
            (window_rect.right - window_rect.left) + width_gap,
            (window_rect.bottom - window_rect.top) + height_gap,
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
        );
    }
}

/// 把主窗从托盘唤回：沿用最大化状态置前，但不抢焦点。
///
/// 置脏与显示绑在一起：不可见窗口收不到 `WM_PAINT`，隐藏期间又没有别的路径
/// 会 `notify` 这个窗口，只 `ShowWindow` 会被 `invalidator.is_dirty()` 拦下，
/// 屏幕上残留的仍是隐藏前那一帧。放在这里是为了让调用方无法漏掉这一步。
#[cfg(windows)]
pub fn show_main_window(window: &mut gpui_kit::Window) {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsZoomed, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE, SetForegroundWindow, ShowWindow,
    };

    window.refresh();
    let Some(hwnd) = hwnd(window) else {
        return;
    };
    unsafe {
        let show_command = if IsZoomed(hwnd).as_bool() {
            SW_SHOWMAXIMIZED
        } else {
            SW_SHOWNOACTIVATE
        };
        let _ = ShowWindow(hwnd, show_command);
        let _ = SetForegroundWindow(hwnd);
    }
}

#[cfg(not(windows))]
pub fn show_main_window(window: &mut gpui_kit::Window) {
    window.refresh();
}

/// 关闭按钮的行为：把主窗从屏幕上撤下，但不销毁窗口。
///
/// `ShowWindow(SW_HIDE)` 走的是 gpui 的 `WM_SHOWWINDOW` 分支，而该分支只在
/// 变为可见时（`wparam == 1`）动作，隐藏方向什么都不做——所以在
/// `on_window_should_close` 回调里同步调用不会重入 App 借用。
#[cfg(windows)]
pub fn hide_main_window(window: &gpui_kit::Window) {
    use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

    if let Some(hwnd) = hwnd(window) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
pub fn hide_main_window(_: &gpui_kit::Window) {}

/// 弹出系统「打开文件」对话框，让用户选一张图片；取消时返回 `None`。
#[cfg(windows)]
pub fn pick_image_file() -> Option<std::path::PathBuf> {
    use windows::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    use windows::core::{PCWSTR, PWSTR};

    // 过滤器是「说明\0通配符\0」序列，末尾再加一个 \0 收尾。
    let filter: Vec<u16> = "图片 (*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp;*.tif;*.tiff;*.ico)\0*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp;*.tif;*.tiff;*.ico\0所有文件 (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect();
    let mut buffer = vec![0u16; 260];
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buffer.as_mut_ptr()),
        nMaxFile: buffer.len() as u32,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };
    let chosen = unsafe { GetOpenFileNameW(&mut ofn).as_bool() };
    if !chosen {
        return None;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(std::path::PathBuf::from(String::from_utf16_lossy(
        &buffer[..len],
    )))
}

#[cfg(not(windows))]
pub fn pick_image_file() -> Option<std::path::PathBuf> {
    None
}
