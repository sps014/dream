//! Relays "any window has focus" from the window capability to the app capability, which
//! cannot link each other directly. Without a registered sink the report is dropped.

use std::sync::Mutex;

type Sink = extern "C" fn(i32);

static SINK: Mutex<Option<Sink>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_set_activity_sink(sink: Option<Sink>) {
    *dream_host_abi::lock(&SINK) = sink;
}

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_report_activity(active: i32) {
    let sink = *dream_host_abi::lock(&SINK);
    if let Some(sink) = sink {
        sink(active);
    }
}
