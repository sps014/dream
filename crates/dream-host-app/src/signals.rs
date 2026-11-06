//! Ctrl+C and termination requests. Outside an active run the default behavior is kept, so a
//! program that finished `Application.run` still stops on Ctrl+C.

use crate::events;
use crate::queue;

#[cfg(unix)]
pub fn install() {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
    use signal_hook::iterator::Signals;
    let Ok(mut signals) = Signals::new([SIGINT, SIGTERM, SIGHUP]) else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("dream-app-signals".into())
        .spawn(move || {
            for signal in signals.forever() {
                if !queue::is_running() {
                    let _ = signal_hook::low_level::emulate_default_handler(signal);
                    continue;
                }
                let (kind, code) = if signal == SIGINT {
                    (events::INTERRUPT, 130)
                } else {
                    (events::TERMINATE, 143)
                };
                if queue::note_stop_signal() > 1 {
                    std::process::exit(code);
                }
                queue::post(kind, 0);
            }
        });
}

#[cfg(windows)]
pub fn install() {
    use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
    unsafe {
        SetConsoleCtrlHandler(Some(console_event), 1);
    }
}

#[cfg(windows)]
unsafe extern "system" fn console_event(event: u32) -> windows_sys::core::BOOL {
    use windows_sys::Win32::System::Console::{
        CTRL_C_EVENT, CTRL_CLOSE_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
    };
    if !queue::is_running() {
        return 0;
    }
    let (kind, code) = if event == CTRL_C_EVENT {
        (events::INTERRUPT, 130)
    } else {
        (events::TERMINATE, 143)
    };
    if queue::note_stop_signal() > 1 {
        std::process::exit(code);
    }
    queue::post(kind, 0);
    // Windows ends the process as soon as these handlers return, so hold it for cleanups.
    if matches!(
        event,
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT
    ) {
        queue::wait_for_stop();
    }
    1
}

#[cfg(not(any(unix, windows)))]
pub fn install() {}
