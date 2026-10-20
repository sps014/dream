//! Native audio output. Guest calls serialize graph mutations; callbacks stay in C.
use dream_host_abi::{DreamPtr, alloc_f32s, alloc_string, read_f32s, read_string};
use std::collections::BTreeMap;
use std::ffi::{CStr, c_char, c_void};
use std::sync::Mutex;

unsafe extern "C" {
    fn dream_audio_output_create(
        samples: *const f32,
        count: i32,
        rate: i32,
        channels: i32,
    ) -> *mut c_void;
    fn dream_audio_output_stream(rate: i32, channels: i32, frames: i32) -> *mut c_void;
    fn dream_audio_output_write(player: *mut c_void, samples: *const f32, count: i32) -> i32;
    fn dream_audio_output_close(player: *mut c_void);
    fn dream_audio_output_control(player: *mut c_void, command: i32, value: f64) -> i32;
    fn dream_audio_output_query(player: *mut c_void, field: i32) -> f64;
    fn dream_audio_output_capture(player: *mut c_void, frames: i32, result: *mut f32) -> i32;
    fn dream_audio_output_sink(player: *mut c_void, index: i32) -> i32;
    fn dream_audio_device_count() -> i32;
    fn dream_audio_device_name(index: i32) -> *const c_char;
    fn dream_audio_device_default(index: i32) -> i32;
    fn dream_audio_context_idle();
}
struct Player(*mut c_void);
// All control access is serialized by PLAYERS; miniaudio synchronizes its audio threads.
unsafe impl Send for Player {}
struct Players {
    next: i32,
    active: BTreeMap<i32, Player>,
}
static PLAYERS: Mutex<Players> = Mutex::new(Players {
    next: 1,
    active: BTreeMap::new(),
});

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_audio_abi_v3() {}

#[unsafe(no_mangle)]
pub extern "C" fn audioMediaCreate(_src: DreamPtr) -> i32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn audioMediaSeek(_handle: i32, _time: f64) -> i32 {
    -1
}

/// # Safety
/// `samples` is a retained guest float array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn audioOutputCreate(samples: DreamPtr, rate: i32, channels: i32) -> i32 {
    let samples = unsafe { read_f32s(samples) };
    let Ok(count) = i32::try_from(samples.len()) else {
        return 0;
    };
    let Ok(mut players) = PLAYERS.lock() else {
        return 0;
    };
    if players.next == i32::MAX {
        return 0;
    }
    let player = unsafe { dream_audio_output_create(samples.as_ptr(), count, rate, channels) };
    register(&mut players, player)
}

fn register(players: &mut Players, player: *mut c_void) -> i32 {
    if player.is_null() {
        return 0;
    }
    let handle = players.next;
    players.next += 1;
    players.active.insert(handle, Player(player));
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn audioOutputStream(rate: i32, channels: i32, frames: i32) -> i32 {
    let Ok(mut players) = PLAYERS.lock() else {
        return 0;
    };
    if players.next == i32::MAX {
        return 0;
    }
    let player = unsafe { dream_audio_output_stream(rate, channels, frames) };
    register(&mut players, player)
}

/// # Safety
/// `samples` is a retained guest float array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn audioOutputWrite(handle: i32, samples: DreamPtr) -> i32 {
    let samples = unsafe { read_f32s(samples) };
    let Ok(count) = i32::try_from(samples.len()) else {
        return 0;
    };
    let Ok(players) = PLAYERS.lock() else {
        return 0;
    };
    players.active.get(&handle).map_or(0, |p| unsafe {
        dream_audio_output_write(p.0, samples.as_ptr(), count)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn audioOutputControl(handle: i32, command: i32, value: f64) -> i32 {
    if !value.is_finite() || handle <= 0 {
        return 0;
    }
    let Ok(players) = PLAYERS.lock() else {
        return 0;
    };
    players.active.get(&handle).map_or(0, |p| unsafe {
        dream_audio_output_control(p.0, command, value)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn audioOutputQuery(handle: i32, field: i32) -> f64 {
    if handle <= 0 {
        return 0.0;
    }
    let Ok(players) = PLAYERS.lock() else {
        return 0.0;
    };
    players
        .active
        .get(&handle)
        .map_or(0.0, |p| unsafe { dream_audio_output_query(p.0, field) })
}

#[unsafe(no_mangle)]
pub extern "C" fn audioOutputClose(handle: i32) {
    if handle <= 0 {
        return;
    }
    let Ok(mut players) = PLAYERS.lock() else {
        return;
    };
    if let Some(player) = players.active.remove(&handle) {
        unsafe { dream_audio_output_close(player.0) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn audioOutputCapture(handle: i32, frames: i32) -> DreamPtr {
    if handle <= 0 || !(0..=32768).contains(&frames) {
        return alloc_f32s(&[]);
    }
    let Ok(players) = PLAYERS.lock() else {
        return alloc_f32s(&[]);
    };
    let Some(player) = players.active.get(&handle) else {
        return alloc_f32s(&[]);
    };
    let mut result = vec![0.0; 5 + frames as usize * 2];
    let count = unsafe { dream_audio_output_capture(player.0, frames, result.as_mut_ptr()) };
    alloc_f32s(&result[..count.max(0) as usize])
}

fn devices() -> Vec<serde_json::Value> {
    let devices = (0..unsafe { dream_audio_device_count() })
        .filter_map(|index| {
            let ptr = unsafe { dream_audio_device_name(index) };
            if ptr.is_null() {
                return None;
            }
            let name = unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned();
            Some(
                serde_json::json!({"id": format!("native:{index}:{name}"), "name": name,
            "is_default": unsafe { dream_audio_device_default(index) } != 0}),
            )
        })
        .collect();
    unsafe { dream_audio_context_idle() };
    devices
}

#[unsafe(no_mangle)]
pub extern "C" fn audioOutputDevices() -> DreamPtr {
    let Ok(_players) = PLAYERS.lock() else {
        return alloc_string("[]");
    };
    alloc_string(&serde_json::Value::Array(devices()).to_string())
}

/// # Safety
/// `id` is a retained guest string for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn audioOutputSink(handle: i32, id: DreamPtr) -> i32 {
    if handle <= 0 {
        return 0;
    }
    let id = unsafe { read_string(id) };
    let Ok(players) = PLAYERS.lock() else {
        return 0;
    };
    let index = if id.is_empty() {
        -1
    } else {
        let Some(index) = devices()
            .iter()
            .position(|device| device["id"].as_str() == Some(&id))
        else {
            return 0;
        };
        index as i32
    };
    players
        .active
        .get(&handle)
        .map_or(0, |p| unsafe { dream_audio_output_sink(p.0, index) })
}

#[unsafe(no_mangle)]
pub extern "C" fn audioOutputStart(handle: i32) -> i32 {
    audioOutputControl(handle, 1, 0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn audioOutputRequest() -> DreamPtr {
    alloc_string("[]")
}
