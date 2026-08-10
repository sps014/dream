//! wgpu device init.

use super::error::{classify_err, note_uncaptured};
use super::state::{lock_state, ERR_UNAVAILABLE};

pub fn is_available() -> bool {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }));
    adapter.is_some()
}

pub fn try_init() -> i32 {
    let mut st = lock_state();
    if st.ready && st.device.is_some() {
        return 0;
    }
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    })) {
        Some(a) => a,
        None => return ERR_UNAVAILABLE,
    };
    let (device, queue) = match pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("dream-gpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    )) {
        Ok(pair) => pair,
        Err(e) => {
            let msg = format!("request_device failed: {e}");
            eprintln!("Dream gpuTryInit: {msg}");
            return classify_err(&msg);
        }
    };
    device.on_uncaptured_error(Box::new(|err| {
        note_uncaptured(err.to_string());
    }));
    st.instance = Some(instance);
    st.adapter = Some(adapter);
    st.device = Some(device);
    st.queue = Some(queue);
    st.ready = true;
    st.last_error = None;
    0
}
