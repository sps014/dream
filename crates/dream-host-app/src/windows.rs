//! Windows notifications through a hidden top-level window on a private thread. Power and
//! session broadcasts only reach top-level windows, so a message-only window would miss them.

use crate::{clock, events, facts, queue};
use std::time::Duration;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{
    CreateMemoryResourceNotification, LowMemoryResourceNotification,
    QueryMemoryResourceNotification,
};
use windows_sys::Win32::System::Power::{
    GetSystemPowerStatus, POWERBROADCAST_SETTING, RegisterPowerSettingNotification,
    SYSTEM_POWER_STATUS,
};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows_sys::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
};
use windows_sys::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW, DispatchMessageW, GetMessageW,
    MSG, PBT_APMPOWERSTATUSCHANGE, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE,
    RegisterClassW, SPI_GETHIGHCONTRAST, SystemParametersInfoW, TranslateMessage, WM_ENDSESSION,
    WM_POWERBROADCAST, WM_QUERYENDSESSION, WM_SETTINGCHANGE, WM_WTSSESSION_CHANGE, WNDCLASSW,
    WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};

/// The OS waits for these answers; hooks get this long before the default answer is sent.
const REPLY_LIMIT: Duration = Duration::from_millis(500);
const LOW_MEMORY_POLL: Duration = Duration::from_secs(2);

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn install() {
    let _ = std::thread::Builder::new()
        .name("dream-app-events".into())
        .spawn(|| unsafe { run() });
    let _ = std::thread::Builder::new()
        .name("dream-app-memory".into())
        .spawn(watch_memory);
}

pub fn refresh() {
    facts::seed(facts::THEME, read_theme());
    facts::seed(facts::POWER_SOURCE, read_power_source());
    facts::seed(facts::DISPLAYS_ON, 1);
}

unsafe fn run() {
    unsafe {
        let class_name = wide("DreamAppEvents");
        let instance = GetModuleHandleW(std::ptr::null());
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&class);
        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if window.is_null() {
            return;
        }
        RegisterPowerSettingNotification(
            window,
            &GUID_CONSOLE_DISPLAY_STATE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        );
        WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION);
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_POWERBROADCAST => {
            power_broadcast(wparam as u32, lparam);
            1
        }
        WM_WTSSESSION_CHANGE => {
            match wparam as u32 {
                WTS_SESSION_LOCK => facts::report(facts::SESSION_LOCKED, 1),
                WTS_SESSION_UNLOCK => facts::report(facts::SESSION_LOCKED, 0),
                _ => {}
            }
            0
        }
        WM_SETTINGCHANGE => {
            let area = unsafe { setting_area(lparam) };
            match area.as_deref() {
                Some("ImmersiveColorSet") => facts::report(facts::THEME, read_theme()),
                Some("intl") => {
                    queue::post(events::LOCALE, 0);
                }
                _ => {}
            }
            0
        }
        WM_QUERYENDSESSION => {
            let vetoed = queue::post_and_wait(events::SESSION_ENDING, 0, REPLY_LIMIT) == Some(1);
            LRESULT::from(!vetoed)
        }
        WM_ENDSESSION => {
            if wparam != 0 && queue::post(events::SESSION_ENDING, 1) {
                queue::wait_for_stop();
            }
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn power_broadcast(event: u32, lparam: LPARAM) {
    match event {
        PBT_APMSUSPEND => {
            clock::mark_sleep();
            queue::post_and_wait(events::SLEEPING, 0, REPLY_LIMIT);
        }
        PBT_APMRESUMEAUTOMATIC => {
            queue::post(events::WOKE, clock::slept_ms());
        }
        PBT_APMPOWERSTATUSCHANGE => facts::report(facts::POWER_SOURCE, read_power_source()),
        PBT_POWERSETTINGCHANGE if lparam != 0 => {
            let setting = unsafe { &*(lparam as *const POWERBROADCAST_SETTING) };
            if guid_eq(&setting.PowerSetting, &GUID_CONSOLE_DISPLAY_STATE)
                && setting.DataLength >= 1
            {
                // 0 off, 1 on, 2 dimmed; a dimmed display is still on.
                facts::report(facts::DISPLAYS_ON, i32::from(setting.Data[0] != 0));
            }
        }
        _ => {}
    }
}

fn guid_eq(a: &windows_sys::core::GUID, b: &windows_sys::core::GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

unsafe fn setting_area(lparam: LPARAM) -> Option<String> {
    if lparam == 0 {
        return None;
    }
    let text = lparam as *const u16;
    let mut length = 0;
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            text, length,
        )))
    }
}

fn watch_memory() {
    let handle = unsafe { CreateMemoryResourceNotification(LowMemoryResourceNotification) };
    if handle.is_null() {
        return;
    }
    let mut was_low = false;
    loop {
        let mut low = 0;
        let ok = unsafe { QueryMemoryResourceNotification(handle, &mut low) } != 0;
        let is_low = ok && low != 0;
        if is_low && !was_low {
            queue::post(events::LOW_MEMORY, 0);
        }
        was_low = is_low;
        std::thread::sleep(LOW_MEMORY_POLL);
    }
}

fn read_theme() -> i32 {
    unsafe {
        let mut contrast: HIGHCONTRASTW = std::mem::zeroed();
        contrast.cbSize = std::mem::size_of::<HIGHCONTRASTW>() as u32;
        if SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            (&raw mut contrast).cast(),
            0,
        ) != 0
            && contrast.dwFlags & HCF_HIGHCONTRASTON != 0
        {
            return facts::THEME_HIGH_CONTRAST;
        }
        let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let value = wide("AppsUseLightTheme");
        let mut light: u32 = 1;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut light).cast(),
            &mut size,
        );
        if status == 0 && light == 0 {
            facts::THEME_DARK
        } else {
            facts::THEME_LIGHT
        }
    }
}

fn read_power_source() -> i32 {
    let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return -1;
    }
    match status.ACLineStatus {
        0 => facts::POWER_BATTERY,
        1 => facts::POWER_AC,
        _ => -1,
    }
}
