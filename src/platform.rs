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

#[cfg(windows)]
pub fn set_autostart(enabled: bool) {
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
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if result.0 != 0 {
        return;
    }

    unsafe {
        if enabled {
            if let Ok(exe) = std::env::current_exe() {
                let mut value: Vec<u16> = exe.to_string_lossy().encode_utf16().collect();
                value.push(0);
                let _ = RegSetValueExW(
                    key,
                    w!("WaterRemainder"),
                    0,
                    REG_SZ,
                    Some(std::slice::from_raw_parts(
                        value.as_ptr() as *const u8,
                        value.len() * 2,
                    )),
                );
            }
        } else {
            let _ = RegDeleteValueW(key, w!("WaterRemainder"));
        }
        let _ = RegCloseKey(key);
    }
}
#[cfg(not(windows))]
pub fn set_autostart(_: bool) {}

#[cfg(windows)]
pub fn ensure_single_instance() -> bool {
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::core::w;
    unsafe {
        match CreateMutexW(None, true, w!("Local\\WaterRemainder.SingleInstance")) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    false
                } else {
                    let _mutex_handle = handle;
                    true
                }
            }
            Err(_) => true,
        }
    }
}

#[cfg(not(windows))]
pub fn ensure_single_instance() -> bool {
    true
}

#[cfg(windows)]
pub fn strip_win11_chrome(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
        DwmSetWindowAttribute,
    };
    unsafe {
        let corner = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        );
        let border = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border as *const _ as *const _,
            std::mem::size_of_val(&border) as u32,
        );
    }
}

#[cfg(windows)]
pub fn style_main_window(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
        DwmSetWindowAttribute,
    };

    unsafe {
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        );
        let border = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border as *const _ as *const _,
            std::mem::size_of_val(&border) as u32,
        );
    }
}

#[cfg(windows)]
pub fn show_main_window(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{
        IsZoomed, SW_SHOWMAXIMIZED, SW_SHOWNOACTIVATE, SetForegroundWindow, ShowWindow,
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
