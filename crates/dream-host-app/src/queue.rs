//! The process-wide event channel. Any thread may post; only the guest's main scheduler
//! receives, one `appNextEvent` future at a time.

use crate::events::Events;
use dream_host_abi::{ForeignFuture, complete_foreign_future, lock};
use std::sync::{Condvar, Mutex};
use std::time::Duration;
#[cfg(any(target_os = "macos", windows))]
use std::time::Instant;

struct Channel {
    running: bool,
    events: Events,
    waiter: Option<ForeignFuture>,
    value: i32,
    awaiting_reply: bool,
    reply: Option<i32>,
    shutdown_timeout: Duration,
    stop_signals: u32,
}

static CHANNEL: Mutex<Channel> = Mutex::new(Channel {
    running: false,
    events: Events::new(),
    waiter: None,
    value: 0,
    awaiting_reply: false,
    reply: None,
    shutdown_timeout: Duration::from_secs(5),
    stop_signals: 0,
});
static CHANGED: Condvar = Condvar::new();

pub fn start(shutdown_timeout_ms: i32) -> bool {
    let mut channel = lock(&CHANNEL);
    if channel.running {
        return false;
    }
    channel.running = true;
    channel.events.clear();
    channel.stop_signals = 0;
    channel.shutdown_timeout = Duration::from_millis(shutdown_timeout_ms.max(0) as u64);
    true
}

pub fn stop() {
    let waiter = {
        let mut channel = lock(&CHANNEL);
        channel.running = false;
        channel.events.clear();
        channel.awaiting_reply = false;
        channel.waiter.take()
    };
    CHANGED.notify_all();
    if let Some(future) = waiter {
        complete_foreign_future(future, 0);
    }
}

pub fn is_running() -> bool {
    lock(&CHANNEL).running
}

/// Counts a stop signal in the current run, so a repeated Ctrl+C can exit at once.
pub fn note_stop_signal() -> u32 {
    let mut channel = lock(&CHANNEL);
    channel.stop_signals += 1;
    channel.stop_signals
}

pub fn next(future: ForeignFuture) {
    let mut channel = lock(&CHANNEL);
    if !channel.running {
        drop(channel);
        complete_foreign_future(future, 0);
        return;
    }
    match channel.events.pop() {
        Some((kind, value)) => {
            channel.value = value;
            drop(channel);
            complete_foreign_future(future, kind as u64);
        }
        None => channel.waiter = Some(future),
    }
}

pub fn value() -> i32 {
    lock(&CHANNEL).value
}

pub fn post(kind: i32, value: i32) -> bool {
    let mut channel = lock(&CHANNEL);
    if !channel.running {
        return false;
    }
    if let Some(future) = channel.waiter.take() {
        channel.value = value;
        drop(channel);
        complete_foreign_future(future, kind as u64);
        return true;
    }
    channel.events.push(kind, value)
}

/// Posts an event whose outcome the operating system is waiting for, then blocks the calling
/// platform thread until the guest replies, the run stops, or `limit` passes.
#[cfg(any(target_os = "macos", windows))]
pub fn post_and_wait(kind: i32, value: i32, limit: Duration) -> Option<i32> {
    {
        let mut channel = lock(&CHANNEL);
        if !channel.running {
            return None;
        }
        channel.awaiting_reply = true;
        channel.reply = None;
    }
    if !post(kind, value) {
        lock(&CHANNEL).awaiting_reply = false;
        return None;
    }
    let deadline = Instant::now() + limit;
    let mut channel = lock(&CHANNEL);
    while channel.running && channel.reply.is_none() {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        channel = CHANGED
            .wait_timeout(channel, deadline - now)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
    channel.awaiting_reply = false;
    channel.reply.take()
}

pub fn reply(value: i32) {
    let mut channel = lock(&CHANNEL);
    if channel.awaiting_reply {
        channel.reply = Some(value);
        drop(channel);
        CHANGED.notify_all();
    }
}

/// Blocks a platform thread that must not return before cleanups finish (a final session-end
/// notice or a console close) until the run stops or the shutdown timeout passes.
#[cfg(any(target_os = "macos", windows))]
pub fn wait_for_stop() {
    let mut channel = lock(&CHANNEL);
    let deadline = Instant::now() + channel.shutdown_timeout;
    while channel.running {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        channel = CHANGED
            .wait_timeout(channel, deadline - now)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}
