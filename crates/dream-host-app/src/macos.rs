//! macOS notifications on a private CFRunLoop thread. IOKit system-power callbacks work
//! without AppKit, so command-line tools and servers get sleep, wake, and power-off too.

use crate::{clock, events, facts, queue};
use core_foundation_sys::base::{
    CFComparisonResult, CFCopyDescription, CFGetTypeID, CFIndex, CFRelease, CFTypeRef,
    kCFAllocatorDefault,
};
use core_foundation_sys::date::CFAbsoluteTimeGetCurrent;
use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{CFBooleanGetTypeID, CFBooleanGetValue, CFBooleanRef};
use core_foundation_sys::preferences::{
    CFPreferencesAppSynchronize, CFPreferencesCopyAppValue, kCFPreferencesAnyApplication,
};
use core_foundation_sys::runloop::{
    CFRunLoopAddSource, CFRunLoopAddTimer, CFRunLoopGetCurrent, CFRunLoopRun, CFRunLoopSourceRef,
    CFRunLoopTimerCreate, CFRunLoopTimerRef, kCFRunLoopDefaultMode,
};
use core_foundation_sys::string::{
    CFStringCompare, CFStringCreateWithCString, CFStringGetCString, CFStringGetTypeID, CFStringRef,
    kCFStringEncodingUTF8,
};
use std::ffi::{CStr, c_void};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

type IoObject = u32;
type IoConnect = u32;
type IoNotificationPort = *mut c_void;
type IoServiceInterestCallback = extern "C" fn(*mut c_void, IoObject, u32, *mut c_void);

const fn common_message(code: u32) -> u32 {
    0xe000_0000 | code
}
const DEVICE_WILL_POWER_OFF: u32 = common_message(0x210);
const DEVICE_HAS_POWERED_ON: u32 = common_message(0x230);
const SYSTEM_WILL_POWER_OFF: u32 = common_message(0x250);
const CAN_SYSTEM_SLEEP: u32 = common_message(0x270);
const SYSTEM_WILL_SLEEP: u32 = common_message(0x280);
const SYSTEM_HAS_POWERED_ON: u32 = common_message(0x300);
const SYSTEM_WILL_RESTART: u32 = common_message(0x310);
const MEMORY_PRESSURE_WARN: usize = 0x2;
const MEMORY_PRESSURE_CRITICAL: usize = 0x4;

/// The OS waits for sleep acknowledgement; hooks get this long before it is sent anyway.
const SLEEP_ACK_LIMIT: Duration = Duration::from_millis(500);

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IORegisterForSystemPower(
        refcon: *mut c_void,
        port: *mut IoNotificationPort,
        callback: IoServiceInterestCallback,
        notifier: *mut IoObject,
    ) -> IoConnect;
    fn IOAllowPowerChange(kernel_port: IoConnect, notification_id: isize) -> i32;
    fn IONotificationPortCreate(main_port: u32) -> IoNotificationPort;
    fn IONotificationPortGetRunLoopSource(port: IoNotificationPort) -> CFRunLoopSourceRef;
    fn IOServiceMatching(name: *const i8) -> *mut c_void;
    fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> IoObject;
    fn IOServiceAddInterestNotification(
        port: IoNotificationPort,
        service: IoObject,
        interest_type: *const i8,
        callback: IoServiceInterestCallback,
        refcon: *mut c_void,
        notification: *mut IoObject,
    ) -> i32;
    fn IOPSNotificationCreateRunLoopSource(
        callback: extern "C" fn(*mut c_void),
        context: *mut c_void,
    ) -> CFRunLoopSourceRef;
    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    fn IOPSGetProvidingPowerSourceType(snapshot: CFTypeRef) -> CFStringRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
}

unsafe extern "C" {
    static _dispatch_source_type_memorypressure: c_void;
    fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut c_void;
    fn dispatch_source_create(
        kind: *const c_void,
        handle: usize,
        mask: usize,
        queue: *mut c_void,
    ) -> *mut c_void;
    fn dispatch_source_set_event_handler_f(
        source: *mut c_void,
        handler: extern "C" fn(*mut c_void),
    );
    fn dispatch_resume(object: *mut c_void);
}

static ROOT_PORT: AtomicU32 = AtomicU32::new(0);

static LOCALE: Mutex<String> = Mutex::new(String::new());
const POLL_INTERVAL_SECONDS: f64 = 2.0;

fn cf_string(text: &CStr) -> CFStringRef {
    unsafe { CFStringCreateWithCString(kCFAllocatorDefault, text.as_ptr(), kCFStringEncodingUTF8) }
}

fn strings_equal(value: CFStringRef, text: &CStr) -> bool {
    let other = cf_string(text);
    let equal = unsafe {
        matches!(
            CFStringCompare(value, other, 0),
            CFComparisonResult::EqualTo
        )
    };
    unsafe { CFRelease(other.cast()) };
    equal
}

pub fn install() {
    let _ = std::thread::Builder::new()
        .name("dream-app-events".into())
        .spawn(|| unsafe { run() });
    watch_memory_pressure();
}

pub fn refresh() {
    facts::seed(facts::THEME, read_theme());
    facts::seed(facts::POWER_SOURCE, read_power_source());
    if let Some(locked) = read_session_locked() {
        facts::seed(facts::SESSION_LOCKED, i32::from(locked));
    }
    facts::seed(facts::DISPLAYS_ON, 1);
    *dream_host_abi::lock(&LOCALE) = read_locale();
}

unsafe fn run() {
    unsafe {
        let run_loop = CFRunLoopGetCurrent();

        let mut power_port: IoNotificationPort = std::ptr::null_mut();
        let mut power_notifier: IoObject = 0;
        let root = IORegisterForSystemPower(
            std::ptr::null_mut(),
            &mut power_port,
            system_power,
            &mut power_notifier,
        );
        if root != 0 {
            ROOT_PORT.store(root, Ordering::SeqCst);
            let source = IONotificationPortGetRunLoopSource(power_port);
            CFRunLoopAddSource(run_loop, source, kCFRunLoopDefaultMode);
        }

        let wrangler =
            IOServiceGetMatchingService(0, IOServiceMatching(c"IODisplayWrangler".as_ptr()));
        if wrangler != 0 {
            let display_port = IONotificationPortCreate(0);
            let mut display_notifier: IoObject = 0;
            let status = IOServiceAddInterestNotification(
                display_port,
                wrangler,
                c"IOGeneralInterest".as_ptr(),
                display_power,
                std::ptr::null_mut(),
                &mut display_notifier,
            );
            if status == 0 {
                let source = IONotificationPortGetRunLoopSource(display_port);
                CFRunLoopAddSource(run_loop, source, kCFRunLoopDefaultMode);
            }
        }

        let power_source =
            IOPSNotificationCreateRunLoopSource(power_source_changed, std::ptr::null_mut());
        if !power_source.is_null() {
            CFRunLoopAddSource(run_loop, power_source, kCFRunLoopDefaultMode);
        }

        let timer = CFRunLoopTimerCreate(
            kCFAllocatorDefault,
            CFAbsoluteTimeGetCurrent() + POLL_INTERVAL_SECONDS,
            POLL_INTERVAL_SECONDS,
            0,
            0,
            poll_preferences,
            std::ptr::null_mut(),
        );
        CFRunLoopAddTimer(run_loop, timer, kCFRunLoopDefaultMode);

        CFRunLoopRun();
    }
}

// Distributed notifications (screen lock, theme, language) are only delivered to the main
// thread's run loop, which a program without windows never runs, so these are re-read here.
extern "C" fn poll_preferences(_timer: CFRunLoopTimerRef, _info: *mut c_void) {
    if !queue::is_running() {
        return;
    }
    if let Some(locked) = read_session_locked() {
        facts::report(facts::SESSION_LOCKED, i32::from(locked));
    }
    facts::report(facts::THEME, read_theme());
    let locale = read_locale();
    let mut last = dream_host_abi::lock(&LOCALE);
    if *last != locale {
        let changed = !last.is_empty();
        *last = locale;
        drop(last);
        if changed {
            queue::post(events::LOCALE, 0);
        }
    }
}

extern "C" fn system_power(
    _refcon: *mut c_void,
    _service: IoObject,
    message: u32,
    argument: *mut c_void,
) {
    let root = ROOT_PORT.load(Ordering::SeqCst);
    match message {
        CAN_SYSTEM_SLEEP => unsafe {
            IOAllowPowerChange(root, argument as isize);
        },
        SYSTEM_WILL_SLEEP => {
            clock::mark_sleep();
            queue::post_and_wait(events::SLEEPING, 0, SLEEP_ACK_LIMIT);
            unsafe { IOAllowPowerChange(root, argument as isize) };
        }
        SYSTEM_HAS_POWERED_ON => {
            queue::post(events::WOKE, clock::slept_ms());
        }
        SYSTEM_WILL_POWER_OFF | SYSTEM_WILL_RESTART if queue::post(events::SESSION_ENDING, 1) => {
            queue::wait_for_stop();
        }
        _ => {}
    }
}

extern "C" fn display_power(
    _refcon: *mut c_void,
    _service: IoObject,
    message: u32,
    _argument: *mut c_void,
) {
    match message {
        DEVICE_WILL_POWER_OFF => facts::report(facts::DISPLAYS_ON, 0),
        DEVICE_HAS_POWERED_ON => facts::report(facts::DISPLAYS_ON, 1),
        _ => {}
    }
}

extern "C" fn power_source_changed(_context: *mut c_void) {
    facts::report(facts::POWER_SOURCE, read_power_source());
}

extern "C" fn memory_pressure(_context: *mut c_void) {
    queue::post(events::LOW_MEMORY, 0);
}

fn watch_memory_pressure() {
    unsafe {
        let source = dispatch_source_create(
            (&raw const _dispatch_source_type_memorypressure).cast(),
            0,
            MEMORY_PRESSURE_WARN | MEMORY_PRESSURE_CRITICAL,
            dispatch_get_global_queue(0, 0),
        );
        if !source.is_null() {
            dispatch_source_set_event_handler_f(source, memory_pressure);
            dispatch_resume(source);
        }
    }
}

fn read_theme() -> i32 {
    unsafe {
        CFPreferencesAppSynchronize(kCFPreferencesAnyApplication);
        let key = cf_string(c"AppleInterfaceStyle");
        let value = CFPreferencesCopyAppValue(key, kCFPreferencesAnyApplication);
        CFRelease(key.cast());
        if value.is_null() {
            return facts::THEME_LIGHT;
        }
        let dark = CFGetTypeID(value) == CFStringGetTypeID()
            && strings_equal(value as CFStringRef, c"Dark");
        CFRelease(value);
        if dark {
            facts::THEME_DARK
        } else {
            facts::THEME_LIGHT
        }
    }
}

/// The preferred languages and region, compared between polls to detect a locale change.
fn read_locale() -> String {
    let mut signature = String::new();
    for name in [c"AppleLanguages", c"AppleLocale"] {
        unsafe {
            let key = cf_string(name);
            let value = CFPreferencesCopyAppValue(key, kCFPreferencesAnyApplication);
            CFRelease(key.cast());
            if value.is_null() {
                continue;
            }
            let description = CFCopyDescription(value);
            CFRelease(value);
            let mut buffer = [0u8; 1024];
            if CFStringGetCString(
                description,
                buffer.as_mut_ptr().cast(),
                buffer.len() as CFIndex,
                kCFStringEncodingUTF8,
            ) != 0
            {
                let text = CStr::from_bytes_until_nul(&buffer).unwrap_or_default();
                signature.push_str(&text.to_string_lossy());
            }
            CFRelease(description.cast());
        }
    }
    signature
}

fn read_power_source() -> i32 {
    unsafe {
        let info = IOPSCopyPowerSourcesInfo();
        if info.is_null() {
            return -1;
        }
        let kind = IOPSGetProvidingPowerSourceType(info);
        let source = if kind.is_null() {
            -1
        } else if strings_equal(kind, c"AC Power") {
            facts::POWER_AC
        } else {
            facts::POWER_BATTERY
        };
        CFRelease(info);
        source
    }
}

fn read_session_locked() -> Option<bool> {
    unsafe {
        let session = CGSessionCopyCurrentDictionary();
        if session.is_null() {
            return None;
        }
        let flag = |name: &CStr| {
            let key = cf_string(name);
            let value = CFDictionaryGetValue(session, key.cast());
            CFRelease(key.cast());
            (!value.is_null() && CFGetTypeID(value) == CFBooleanGetTypeID())
                .then(|| CFBooleanGetValue(value as CFBooleanRef))
        };
        let locked = flag(c"CGSSessionScreenIsLocked").unwrap_or(false);
        let on_console = flag(c"kCGSSessionOnConsoleKey").unwrap_or(true);
        CFRelease(session.cast());
        Some(locked || !on_console)
    }
}
