//! Window surfaces. The window host owns the native window and its thread; this host asks it
//! for raw handles through `dream_window_surface_v1` and never calls GLFW. Every call here runs
//! on the guest's main thread, where the window host requires them.

use crate::error::{self, Code, HostError, HostResult, check};
use crate::ffi;
use crate::state::{self, Entry, Kind, Obj, Surface, SurfaceEntry, TextureEntry};
use crate::waiter;
use dream_host_abi::window_surface::{WINDOW_SURFACE_SYMBOL, WindowSurface};
use std::sync::OnceLock;

pub type SurfaceFn = unsafe extern "C" fn(i32, *mut WindowSurface) -> i32;

static OVERRIDE: OnceLock<SurfaceFn> = OnceLock::new();

/// Host tests link the window host statically and hand its export over directly.
pub fn set_window_surface_fn(f: SurfaceFn) {
    let _ = OVERRIDE.set(f);
}

/// The window host's export, found among the libraries the program already loaded.
fn window_surface_fn() -> Option<SurfaceFn> {
    if let Some(f) = OVERRIDE.get() {
        return Some(*f);
    }
    let symbol = std::ffi::CString::new(WINDOW_SURFACE_SYMBOL).ok()?;
    let address = lookup(&symbol);
    (!address.is_null()).then(|| unsafe { std::mem::transmute::<*mut std::ffi::c_void, SurfaceFn>(address) })
}

#[cfg(unix)]
fn lookup(symbol: &std::ffi::CStr) -> *mut std::ffi::c_void {
    unsafe { libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr()) }
}

#[cfg(windows)]
fn lookup(symbol: &std::ffi::CStr) -> *mut std::ffi::c_void {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    let module = unsafe { GetModuleHandleA(c"dream_host_window.dll".as_ptr().cast()) };
    if module.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { GetProcAddress(module, symbol.as_ptr().cast()) }
        .map_or(std::ptr::null_mut(), |f| f as *mut std::ffi::c_void)
}

fn configure(surface: &Surface, width: u32, height: u32, vsync: bool) -> HostResult<u32> {
    let mut format = 0;
    check(unsafe { ffi::dgpu_surface_configure(surface.raw(), width, height, vsync as i32, &mut format) })?;
    Ok(format)
}

pub fn surface_create(device: i32, window: i32, vsync: bool) -> HostResult<i32> {
    let surface_fn = window_surface_fn().ok_or_else(|| {
        HostError::new(Code::Surface, "the window host is not loaded in this program")
    })?;
    let mut native = WindowSurface::default();
    if unsafe { surface_fn(window, &mut native) } == 0 {
        return Err(HostError::new(
            Code::Surface,
            "the window is headless or closed, or the call ran off the main thread",
        ));
    }
    let mut state = state::lock();
    let core = state.device(device)?;
    // `c_ulong` is 32 bits on Windows and 64 bits elsewhere.
    #[allow(clippy::unnecessary_cast)]
    let xlib_window = native.xlib_window as u64;
    let desc = ffi::Window {
        platform: native.platform,
        window: native.window,
        display: native.display,
        xlib_window,
    };
    let mut raw = std::ptr::null_mut();
    check(unsafe { ffi::dgpu_surface_create(core.raw(), &desc, &mut raw) })?;
    let surface = Surface::from_raw(raw);
    let (width, height) = (native.pixel_width.max(1), native.pixel_height.max(1));
    let format = configure(&surface, width, height, vsync)?;
    state.insert(Entry {
        kind: Kind::Surface(SurfaceEntry {
            surface,
            width,
            height,
            format,
            vsync,
            image: None,
        }),
        device,
        last_use: 0,
        core,
    })
}

fn surface_mut(state: &mut state::State, handle: i32) -> HostResult<&mut SurfaceEntry> {
    let entry = state.entry_mut(handle)?;
    entry.core.alive()?;
    match &mut entry.kind {
        Kind::Surface(surface) => Ok(surface),
        other => Err(HostError::invalid(format!("handle {handle} is a {}, not a surface", other.name()))),
    }
}

pub fn surface_resize(handle: i32, width: i32, height: i32) -> HostResult<()> {
    if width <= 0 || height <= 0 {
        return Err(HostError::invalid("surface sizes must be positive"));
    }
    let mut state = state::lock();
    let surface = surface_mut(&mut state, handle)?;
    surface.format = configure(&surface.surface, width as u32, height as u32, surface.vsync)?;
    surface.width = width as u32;
    surface.height = height as u32;
    Ok(())
}

/// [format, width, height].
pub fn surface_info(handle: i32) -> HostResult<[i32; 3]> {
    let mut state = state::lock();
    let surface = surface_mut(&mut state, handle)?;
    Ok([surface.format as i32, surface.width as i32, surface.height as i32])
}

pub fn acquire(future: usize, handle: i32) {
    let value = acquire_now(handle).unwrap_or_else(|e| {
        error::record(&e);
        0
    });
    waiter::complete(future, value as u64);
}

fn acquire_now(handle: i32) -> HostResult<i32> {
    let mut state = state::lock();
    let (device, core) = {
        let entry = state.entry(handle)?;
        (entry.device, entry.core.clone())
    };
    let surface = surface_mut(&mut state, handle)?;
    let previous = surface.image.take();
    let mut raw = std::ptr::null_mut();
    check(unsafe { ffi::dgpu_surface_acquire(surface.surface.raw(), &mut raw) })?;
    let texture = Obj::from_raw(raw);
    if let Some(previous) = previous {
        state.release(previous);
    }
    let image = state.insert(Entry {
        kind: Kind::Texture(TextureEntry {
            obj: texture,
            layers: 1,
            mips: 1,
            storage: false,
        }),
        device,
        last_use: 0,
        core,
    })?;
    surface_mut(&mut state, handle)?.image = Some(image);
    Ok(image)
}

pub fn present(future: usize, handle: i32) {
    let value = match present_now(handle) {
        Ok(()) => 0,
        Err(e) => {
            error::record(&e);
            1
        }
    };
    waiter::complete(future, value);
}

fn present_now(handle: i32) -> HostResult<()> {
    let mut state = state::lock();
    let surface = surface_mut(&mut state, handle)?;
    let Some(image) = surface.image.take() else {
        return Err(HostError::new(Code::Validation, "present without an acquired image"));
    };
    let presented = check(unsafe { ffi::dgpu_surface_present(surface.surface.raw()) });
    state.release(image);
    presented
}
