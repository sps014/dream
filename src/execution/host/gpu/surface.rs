//! Native window surface (winit) + present / blit.

use super::state::{lock_state, SurfaceEntry, ERR_OTHER, ERR_UNAVAILABLE};
use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::{Window, WindowId};

thread_local! {
    // EventLoop is !Send/!Sync — keep it thread-local (dream run is single-threaded).
    static EVENT_LOOP: RefCell<Option<EventLoop<()>>> = const { RefCell::new(None) };
}

struct WindowCreateApp {
    title: String,
    width: u32,
    height: u32,
    window: Option<Arc<Window>>,
}

impl ApplicationHandler for WindowCreateApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.width.max(1) as f64,
                self.height.max(1) as f64,
            ));
        match event_loop.create_window(attrs) {
            Ok(w) => self.window = Some(Arc::new(w)),
            Err(e) => eprintln!("Dream gpuSurfaceCreate: window create failed: {e}"),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
}

struct PumpApp;

impl ApplicationHandler for PumpApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested) {
            event_loop.exit();
        }
    }
}

fn with_event_loop<R>(f: impl FnOnce(&mut EventLoop<()>) -> R) -> Result<R, String> {
    EVENT_LOOP.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(EventLoop::new().map_err(|e| format!("EventLoop: {e}"))?);
        }
        Ok(f(slot.as_mut().unwrap()))
    })
}

/// Legacy: create with default 800×600.
pub fn from_canvas(name: &str) -> i32 {
    create(name, 800, 600)
}

/// Create a surface / native window at `width`×`height`, titled `name`.
pub fn create(name: &str, width: i32, height: i32) -> i32 {
    let mut st = lock_state();
    if !st.ready {
        return -1;
    }
    let id = st.alloc_id();
    let w = width.max(1) as u32;
    let h = height.max(1) as u32;
    let mut entry = SurfaceEntry {
        width: w,
        height: h,
        color: None,
        depth: None,
        window: None,
        surface: None,
        config: None,
        pending_frame: None,
    };

    let title = name.to_string();
    let created = with_event_loop(|el| {
        let mut app = WindowCreateApp {
            title,
            width: w,
            height: h,
            window: None,
        };
        let _ = el.pump_app_events(Some(Duration::ZERO), &mut app);
        app.window
    });

    if let Ok(Some(window)) = created {
        let instance = st.instance.as_ref().unwrap();
        match instance.create_surface(window.clone()) {
            Ok(surface) => {
                if let Some(adapter) = st.adapter.as_ref() {
                    let caps = surface.get_capabilities(adapter);
                    if let Some(fmt) = caps.formats.first().copied() {
                        st.render_format = fmt;
                    }
                }
                entry.window = Some(window);
                entry.surface = Some(surface);
            }
            Err(e) => {
                eprintln!("Dream gpuSurfaceCreate: surface create failed: {e}");
                entry.window = Some(window);
            }
        }
    } else if let Err(e) = created {
        eprintln!("Dream gpuSurfaceCreate: {e}");
    }

    // Auto-configure when a window surface exists so the first draw can hit the swapchain.
    if entry.surface.is_some() {
        let device = st.device.as_ref().unwrap().clone();
        let format = st.render_format;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
            format,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        if let Some(surface) = entry.surface.as_ref() {
            surface.configure(&device, &config);
        }
        entry.config = Some(config);
    }

    st.surfaces.insert(id, entry);
    id
}

pub fn configure(id: i32, width: i32, height: i32) {
    let mut st = lock_state();
    let Some(surf) = st.surfaces.get_mut(&id) else {
        return;
    };
    surf.width = width.max(1) as u32;
    surf.height = height.max(1) as u32;
    surf.color = None;
    surf.depth = None;
    surf.pending_frame = None;

    let (w, h) = (surf.width, surf.height);
    let has_window = surf.window.is_some() && surf.surface.is_some();
    if !has_window {
        return;
    }
    if let Some(window) = surf.window.as_ref() {
        let _ = window.request_inner_size(winit::dpi::LogicalSize::new(w as f64, h as f64));
    }
    let device = match st.device.as_ref() {
        Some(d) => d.clone(),
        None => return,
    };
    let format = st.render_format;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
        format,
        width: w,
        height: h,
        present_mode: wgpu::PresentMode::AutoVsync,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
    };
    if let Some(surface) = st.surfaces.get(&id).and_then(|s| s.surface.as_ref()) {
        surface.configure(&device, &config);
    }
    if let Some(surf) = st.surfaces.get_mut(&id) {
        surf.config = Some(config);
    }
}

pub fn present(id: i32) -> i32 {
    match present_inner(id) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Dream gpuSurfacePresent: {e}");
            if e.contains("unavailable") || e.contains("no window") {
                ERR_UNAVAILABLE
            } else {
                ERR_OTHER
            }
        }
    }
}

fn present_inner(id: i32) -> Result<(), String> {
    let mut st = lock_state();
    if !st.ready {
        return Err("GPU not initialized".into());
    }
    let device = st.device.as_ref().unwrap().clone();
    let queue = st.queue.as_ref().unwrap().clone();

    let surf = st
        .surfaces
        .get_mut(&id)
        .ok_or_else(|| format!("unknown surface {id}"))?;
    let Some(surface) = surf.surface.as_ref() else {
        return Err("no window surface (unavailable)".into());
    };
    if surf.config.is_none() {
        return Err("surface not configured".into());
    }

    // Fast path: last draw already targeted the swapchain.
    if let Some(frame) = surf.pending_frame.take() {
        frame.present();
        let _ = device;
        let _ = queue;
        drop(st);
        let _ = with_event_loop(|el| {
            let mut app = PumpApp;
            let _ = el.pump_app_events(Some(Duration::ZERO), &mut app);
        });
        return Ok(());
    }

    // Slow path: blit offscreen color → swapchain (e.g. after `blit`).
    let width = surf.width;
    let height = surf.height;
    let frame = surface
        .get_current_texture()
        .map_err(|e| format!("acquire: {e}"))?;

    if let Some(color) = surf.color.as_ref() {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("dream-present"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: width.min(frame.texture.width()),
                height: height.min(frame.texture.height()),
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
    }
    frame.present();
    drop(st);

    let _ = with_event_loop(|el| {
        let mut app = PumpApp;
        let _ = el.pump_app_events(Some(Duration::ZERO), &mut app);
    });
    Ok(())
}

pub fn blit(surface_id: i32, texture_id: i32) -> i32 {
    match blit_inner(surface_id, texture_id) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Dream gpuRenderBlit: {e}");
            ERR_OTHER
        }
    }
}

fn blit_inner(surface_id: i32, texture_id: i32) -> Result<(), String> {
    let mut st = lock_state();
    if !st.ready {
        return Err("GPU not initialized".into());
    }
    let device = st.device.as_ref().unwrap().clone();
    let queue = st.queue.as_ref().unwrap().clone();
    let format = st.render_format;

    let tex = st
        .textures
        .get_mut(&texture_id)
        .ok_or_else(|| format!("unknown texture {texture_id}"))?;
    if tex.gpu.is_none() {
        let usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC;
        let gpu = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dream-blit-src"),
            size: wgpu::Extent3d {
                width: tex.width.max(1),
                height: tex.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: tex.format,
            usage,
            view_formats: &[],
        });
        if !tex.cpu.is_empty() && !tex.depth {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &gpu,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &tex.cpu,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(tex.width * 4),
                    rows_per_image: Some(tex.height),
                },
                wgpu::Extent3d {
                    width: tex.width,
                    height: tex.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        tex.gpu = Some(gpu);
    }

    let src = tex.gpu.as_ref().unwrap().clone();
    let surf = st
        .surfaces
        .get_mut(&surface_id)
        .ok_or_else(|| format!("unknown surface {surface_id}"))?;
    // Blit always targets offscreen; present will copy to swapchain.
    surf.pending_frame = None;
    if surf.color.is_none() {
        surf.color = Some(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dream-surface-color"),
            size: wgpu::Extent3d {
                width: surf.width.max(1),
                height: surf.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        }));
    }
    let dst = surf.color.as_ref().unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dream-blit"),
    });
    let w = src.width().min(dst.width());
    let h = src.height().min(dst.height());
    encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &src,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: dst,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    Ok(())
}

pub fn frame_tick() {
    let _ = with_event_loop(|el| {
        let mut app = PumpApp;
        // Vsync already paces present; only pump input, don't sleep.
        let _ = el.pump_app_events(Some(Duration::ZERO), &mut app);
    });
}
