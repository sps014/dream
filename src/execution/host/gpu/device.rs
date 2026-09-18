//! wgpu device init.

use super::error::{classify_err, drain_lost, note_device_lost, note_uncaptured_error};
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

pub fn try_init(power: i32) -> i32 {
    let pref = match power {
        1 => wgpu::PowerPreference::LowPower,
        0 => wgpu::PowerPreference::None,
        _ => wgpu::PowerPreference::HighPerformance,
    };
    let mut st = lock_state();
    if let Some(msg) = drain_lost() {
        st.drop_gpu_device();
        st.set_last_error(msg);
    }
    if st.ready && st.device.is_some() {
        return 0;
    }
    if st.instance.is_none() {
        st.instance = Some(wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        }));
    }
    if st.adapter.is_some() && st.power_preference != pref {
        st.adapter = None;
    }
    st.power_preference = pref;
    if st.adapter.is_none() {
        let instance = st.instance.as_ref().unwrap();
        let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: pref,
            compatible_surface: None,
            force_fallback_adapter: false,
        })) {
            Some(a) => a,
            None => {
                st.set_last_error("no GPU adapter".into());
                return ERR_UNAVAILABLE;
            }
        };
        st.adapter = Some(adapter);
    }
    let adapter = st.adapter.as_ref().unwrap();
    let (device, queue) = match pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("dream-gpu"),
            required_features: super::caps::requested_features(adapter),
            required_limits: super::caps::requested_limits(adapter),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    )) {
        Ok(pair) => pair,
        Err(e) => {
            let msg = format!("request_device failed: {e}");
            eprintln!("Dream gpuTryInit: {msg}");
            st.set_last_error(msg.clone());
            return classify_err(&msg);
        }
    };
    device.on_uncaptured_error(Box::new(|err| {
        note_uncaptured_error(err);
    }));
    device.set_device_lost_callback(move |reason, msg| {
        note_device_lost(format!("device lost ({reason:?}): {msg}"));
    });
    st.device = Some(device);
    st.queue = Some(queue);
    st.ready = true;
    st.last_error = None;
    0
}
