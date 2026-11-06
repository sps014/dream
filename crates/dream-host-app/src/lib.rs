//! Native process lifetime for `system.app`: signals and operating-system power, session,
//! theme, and locale notifications. Platform callbacks only enqueue; the guest drains the
//! queue on its scheduler through `appNextEvent`.

mod clock;
mod events;
mod exports;
mod facts;
#[cfg(target_os = "macos")]
mod macos;
mod queue;
mod signals;
#[cfg(windows)]
mod windows;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_app_abi_v3() {}

/// Platform watchers start once, on the first `Application.run`, and outlive it: they only
/// post while a run is active.
fn install_watchers() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        signals::install();
        dream_host_abi::set_app_activity_sink(Some(facts::on_activity));
        #[cfg(target_os = "macos")]
        macos::install();
        #[cfg(windows)]
        windows::install();
    });
}

fn refresh_facts() {
    #[cfg(target_os = "macos")]
    macos::refresh();
    #[cfg(windows)]
    windows::refresh();
}
