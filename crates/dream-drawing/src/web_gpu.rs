use super::{SURFACES, js_error};
use crate::{
    Surface,
    gpu::{GpuContext, GpuSurface},
};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local! {
    static CONTEXTS: RefCell<Vec<Option<GpuContext>>> = const { RefCell::new(Vec::new()) };
}

#[wasm_bindgen]
pub async fn gpu_context() -> Result<JsValue, JsValue> {
    let context = GpuContext::new().await.map_err(js_error)?;
    let raw = context
        .device
        .as_webgpu()
        .ok_or_else(|| js_sys::Error::new("EUNAVAILABLE\nWebGPU device is unavailable"))?
        .clone();
    let handle = CONTEXTS.with(|contexts| {
        let mut contexts = contexts.borrow_mut();
        let handle = i32::try_from(contexts.len() + 1)
            .map_err(|_| js_sys::Error::new("ELIMIT\ndrawing device handles exhausted"))?;
        contexts.push(Some(context));
        Ok::<_, JsValue>(handle)
    })?;
    let result = js_sys::Array::new();
    result.push(&JsValue::from(handle));
    result.push(&raw.into());
    Ok(result.into())
}

#[wasm_bindgen]
pub fn gpu_context_release(handle: u32) {
    CONTEXTS.with(|contexts| {
        if let Some(slot) = contexts
            .borrow_mut()
            .get_mut(handle.wrapping_sub(1) as usize)
        {
            *slot = None;
        }
    });
}

#[wasm_bindgen]
pub fn gpu_source(handle: u32, context: u32) -> Result<JsValue, JsValue> {
    let device = CONTEXTS
        .with(|contexts| {
            contexts
                .borrow()
                .get(context.wrapping_sub(1) as usize)
                .and_then(Option::as_ref)
                .cloned()
        })
        .ok_or_else(|| js_sys::Error::new("EINVAL\nGPU device was released"))?;
    SURFACES.with(|surfaces| {
        let surfaces = surfaces.borrow();
        let mut record = surfaces
            .get(handle.wrapping_sub(1) as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| js_sys::Error::new("EINVAL\ndrawing surface is closed"))?
            .try_borrow_mut()
            .map_err(|_| js_sys::Error::new("EBUSY\ndrawing surface has an outstanding read"))?;
        if !matches!(record.surface, Surface::Gpu(_)) {
            return Err(js_sys::Error::new("EINVAL\nCPU drawings require a pixel upload").into());
        }
        if record.context != context {
            let mut replacement =
                GpuSurface::with_context(record.width, record.height, device).map_err(js_error)?;
            if let Some(picture) = &record.picture {
                replacement.render(picture).map_err(js_error)?;
            }
            record.surface = Surface::Gpu(Box::new(replacement));
            record.context = context;
        }
        let Surface::Gpu(surface) = &record.surface else {
            unreachable!()
        };
        surface
            .target
            .as_webgpu()
            .cloned()
            .map(JsValue::from)
            .ok_or_else(|| js_sys::Error::new("EUNAVAILABLE\nWebGPU texture is unavailable").into())
    })
}
