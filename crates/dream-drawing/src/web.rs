#[path = "web_gpu.rs"]
mod gpu;
use crate::assets::{Asset, RasterImage};
use crate::{DrawingError, Picture, Surface};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

thread_local! {
    static SURFACES: RefCell<Vec<Option<Rc<RefCell<WebSurface>>>>> = const { RefCell::new(Vec::new()) };
    static ASSETS: RefCell<Vec<Option<Asset>>> = const { RefCell::new(Vec::new()) };
}

struct WebSurface {
    surface: Surface,
    picture: Option<Picture>,
    context: u32,
    width: u16,
    height: u16,
}

fn js_error(error: DrawingError) -> JsValue {
    js_sys::Error::new(&error.to_string()).into()
}

#[wasm_bindgen]
pub async fn create(width: i32, height: i32, backend: i32) -> std::result::Result<u32, JsValue> {
    let surface = Surface::create(width, height, backend)
        .await
        .map_err(js_error)?;
    SURFACES.with(|table| {
        let mut table = table.borrow_mut();
        // Never reuse browser handles: a stale reference must not name a new surface.
        let handle = i32::try_from(table.len() + 1)
            .map_err(|_| js_sys::Error::new("ELIMIT\ndrawing handle space exhausted"))?;
        table.push(Some(Rc::new(RefCell::new(WebSurface {
            surface,
            picture: None,
            context: 0,
            width: width as u16,
            height: height as u16,
        }))));
        Ok(handle as u32)
    })
}

#[wasm_bindgen]
pub fn render(handle: u32, values: &[f32], handles: &[i32]) -> std::result::Result<(), JsValue> {
    let assets = ASSETS.with(|table| {
        let table = table.borrow();
        handles
            .iter()
            .map(|handle| {
                table
                    .get(handle.wrapping_sub(1) as usize)
                    .and_then(Option::as_ref)
                    .cloned()
                    .ok_or_else(|| js_sys::Error::new("EINVAL\ndrawing resource was released"))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
    })?;
    let picture = Picture::decode_with(values, &assets).map_err(js_error)?;
    SURFACES.with(|table| {
        let table = table.borrow();
        let mut record = table
            .get(handle.wrapping_sub(1) as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| js_sys::Error::new("EINVAL\ndrawing surface is closed"))?
            .try_borrow_mut()
            .map_err(|_| js_sys::Error::new("EBUSY\ndrawing surface has an outstanding read"))?;
        record.surface.render(&picture).map_err(js_error)?;
        record.picture = Some(picture);
        Ok(())
    })
}

#[wasm_bindgen]
pub async fn pixels(handle: u32) -> std::result::Result<Vec<u8>, JsValue> {
    // Own a reference through the await so closing a surface cannot resurrect its
    // handle. Other surfaces remain usable while this one's read is in flight.
    let surface = SURFACES
        .with(|table| {
            table
                .borrow()
                .get(handle.wrapping_sub(1) as usize)
                .and_then(Option::as_ref)
                .cloned()
        })
        .ok_or_else(|| js_sys::Error::new("EINVAL\ndrawing surface is closed or busy"))?;
    let borrowed = surface
        .try_borrow()
        .map_err(|_| js_sys::Error::new("EBUSY\ndrawing surface is busy"))?;
    borrowed.surface.pixels().await.map_err(js_error)
}

#[wasm_bindgen]
pub fn backend(handle: u32) -> i32 {
    SURFACES.with(|table| {
        table
            .borrow()
            .get(handle.wrapping_sub(1) as usize)
            .and_then(Option::as_ref)
            .map_or(0, |surface| {
                surface
                    .try_borrow()
                    .map_or(0, |surface| surface.surface.backend())
            })
    })
}

#[wasm_bindgen]
pub fn release(handle: u32) {
    SURFACES.with(|table| {
        if let Some(slot) = table.borrow_mut().get_mut(handle.wrapping_sub(1) as usize) {
            *slot = None;
        }
    });
}

fn asset_handle(asset: Asset) -> std::result::Result<i32, JsValue> {
    ASSETS.with(|table| {
        let mut table = table.borrow_mut();
        let handle = i32::try_from(table.len() + 1)
            .map_err(|_| js_sys::Error::new("ELIMIT\ndrawing resource handles exhausted"))?;
        table.push(Some(asset));
        Ok(handle)
    })
}

#[wasm_bindgen]
pub fn image_decode(bytes: &[u8]) -> std::result::Result<i32, JsValue> {
    asset_handle(Asset::Image(RasterImage::decode(bytes).map_err(js_error)?))
}

#[wasm_bindgen]
pub fn image_rgba(width: i32, height: i32, bytes: &[u8]) -> std::result::Result<i32, JsValue> {
    asset_handle(Asset::Image(
        RasterImage::rgba(width, height, bytes).map_err(js_error)?,
    ))
}

#[wasm_bindgen]
pub fn text_create(
    text: &str,
    family: &str,
    size: f32,
    weight: f32,
    width: f32,
    alignment: u32,
) -> std::result::Result<i32, JsValue> {
    asset_handle(Asset::Text(
        crate::text::layout(text, family, size, weight, width, alignment).map_err(js_error)?,
    ))
}

#[wasm_bindgen]
pub fn font_register(bytes: &[u8]) -> std::result::Result<String, JsValue> {
    crate::text::register(bytes).map_err(js_error)
}

#[wasm_bindgen]
pub fn asset_metrics(handle: i32) -> Vec<f32> {
    ASSETS.with(|table| {
        table
            .borrow()
            .get(handle.wrapping_sub(1) as usize)
            .and_then(Option::as_ref)
            .map_or_else(Vec::new, Asset::metrics)
    })
}

#[wasm_bindgen]
pub fn asset_release(handle: i32) {
    ASSETS.with(|table| {
        if let Some(asset) = table.borrow_mut().get_mut(handle.wrapping_sub(1) as usize) {
            *asset = None;
        }
    });
}
