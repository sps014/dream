//! Native drawing capability; guest allocation remains owned by dream-host-core.

#[cfg(windows)]
mod native;

use dream_drawing::assets::{Asset, RasterImage};
use dream_drawing::{Picture, Surface};
use dream_host_abi::{
    DreamPtr, ForeignFuture, alloc_bytes, alloc_f32s, alloc_string, complete_foreign_future,
    handles::HandleTable, read_bytes, read_f32s, read_i32s, read_string,
};
use std::cell::RefCell;
use std::sync::Mutex;

static SURFACES: Mutex<HandleTable<NativeSurface>> = Mutex::new(HandleTable::new());
struct NativeSurface {
    surface: Surface,
    picture: Option<Picture>,
}

impl NativeSurface {
    fn render(&mut self, picture: Picture) -> dream_drawing::Result<()> {
        self.surface.render(&picture)?;
        self.picture = (self.surface.backend() == 2).then_some(picture);
        Ok(())
    }
}

static ASSETS: Mutex<HandleTable<Asset>> = Mutex::new(HandleTable::new());

thread_local! {
    static ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn fail(error: impl ToString) -> i32 {
    ERROR.with(|last| *last.borrow_mut() = error.to_string());
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_drawing_abi_v3() {}

#[unsafe(no_mangle)]
pub extern "C" fn drawingCreateAsync(
    future: DreamPtr,
    width: i32,
    height: i32,
    backend: i32,
) -> i32 {
    let result = pollster::block_on(Surface::create(width, height, backend));
    let handle = match result {
        Ok(surface) => dream_host_abi::lock(&SURFACES)
            .insert(NativeSurface {
                surface,
                picture: None,
            })
            .unwrap_or_else(|| fail("ELIMIT\ndrawing handle table is full")),
        Err(error) => fail(error),
    };
    complete_foreign_future(ForeignFuture::new(future), handle as u64);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn drawingLastError() -> DreamPtr {
    ERROR.with(|last| alloc_string(&last.borrow()))
}

/// # Safety
/// `commands` is a readable guest float array retained for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drawingRender(handle: i32, commands: DreamPtr, assets: DreamPtr) -> i32 {
    let data = unsafe { read_f32s(commands) };
    let handles = unsafe { read_i32s(assets) };
    let resources = {
        let assets = dream_host_abi::lock(&ASSETS);
        handles
            .iter()
            .map(|handle| {
                assets.get(*handle).cloned().ok_or_else(|| {
                    dream_drawing::DrawingError::Invalid("drawing resource was released".into())
                })
            })
            .collect::<dream_drawing::Result<Vec<_>>>()
    };
    let result = resources
        .and_then(|resources| Picture::decode_with(&data, &resources))
        .and_then(|picture| {
            let mut table = dream_host_abi::lock(&SURFACES);
            table
                .get_mut(handle)
                .ok_or_else(|| {
                    dream_drawing::DrawingError::Invalid("drawing surface is closed".into())
                })?
                .render(picture)
        });
    match result {
        Ok(()) => 1,
        Err(error) => fail(error),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn drawingPixelsAsync(future: DreamPtr, handle: i32) -> i32 {
    let result = {
        let table = dream_host_abi::lock(&SURFACES);
        table
            .get(handle)
            .ok_or_else(|| dream_drawing::DrawingError::Invalid("drawing surface is closed".into()))
            .and_then(|surface| pollster::block_on(surface.surface.pixels()))
    };
    let pointer = match result {
        Ok(pixels) => alloc_bytes(&pixels),
        Err(error) => {
            fail(error);
            alloc_bytes(&[])
        }
    };
    complete_foreign_future(ForeignFuture::new(future), pointer as u64);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn drawingCopyGpu(handle: i32, texture: i32, width: i32, height: i32) -> i32 {
    #[cfg(windows)]
    {
        native::copy(handle, texture, width, height)
    }
    #[cfg(not(windows))]
    {
        let _ = (handle, texture, width, height);
        -1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn drawingBackend(handle: i32) -> i32 {
    dream_host_abi::lock(&SURFACES)
        .get(handle)
        .map_or(0, |surface| surface.surface.backend())
}

#[unsafe(no_mangle)]
pub extern "C" fn drawingRelease(handle: i32) {
    dream_host_abi::lock(&SURFACES).remove(handle);
}

fn asset_result(future: DreamPtr, result: dream_drawing::Result<Asset>) -> i32 {
    let handle = match result {
        Ok(asset) => dream_host_abi::lock(&ASSETS)
            .insert(asset)
            .unwrap_or_else(|| fail("ELIMIT\ndrawing resource table is full")),
        Err(error) => fail(error),
    };
    complete_foreign_future(ForeignFuture::new(future), handle as u64);
    1
}

/// # Safety
/// `bytes` is a readable guest byte array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drawingImageDecodeAsync(future: DreamPtr, bytes: DreamPtr) -> i32 {
    asset_result(
        future,
        RasterImage::decode(&unsafe { read_bytes(bytes) }).map(Asset::Image),
    )
}

/// # Safety
/// `bytes` is a readable guest byte array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drawingImageRgbaAsync(
    future: DreamPtr,
    width: i32,
    height: i32,
    bytes: DreamPtr,
) -> i32 {
    asset_result(
        future,
        RasterImage::rgba(width, height, &unsafe { read_bytes(bytes) }).map(Asset::Image),
    )
}

/// # Safety
/// `text` and `family` are readable guest strings retained for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drawingTextCreateAsync(
    future: DreamPtr,
    text: DreamPtr,
    family: DreamPtr,
    size: f32,
    weight: f32,
    width: f32,
    alignment: i32,
) -> i32 {
    let text = unsafe { read_string(text) };
    let family = unsafe { read_string(family) };
    asset_result(
        future,
        dream_drawing::text::layout(&text, &family, size, weight, width, alignment as u32)
            .map(Asset::Text),
    )
}

/// # Safety
/// `bytes` is a readable guest byte array for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn drawingFontRegisterAsync(future: DreamPtr, bytes: DreamPtr) -> i32 {
    let result = dream_drawing::text::register(&unsafe { read_bytes(bytes) });
    let family = match result {
        Ok(family) => family,
        Err(error) => {
            fail(error);
            String::new()
        }
    };
    complete_foreign_future(ForeignFuture::new(future), alloc_string(&family) as u64);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn drawingAssetMetrics(handle: i32) -> DreamPtr {
    let values = dream_host_abi::lock(&ASSETS)
        .get(handle)
        .map(Asset::metrics)
        .unwrap_or_default();
    alloc_f32s(&values)
}

#[unsafe(no_mangle)]
pub extern "C" fn drawingAssetRelease(handle: i32) {
    dream_host_abi::lock(&ASSETS).remove(handle);
}
