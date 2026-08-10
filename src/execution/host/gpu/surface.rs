//! Native window surface (winit) + present / blit.

use super::error::{classify_err, classify_surface_error, drain_uncaptured};
use super::state::{lock_state, SurfaceEntry};
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
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        dispatch_window_event(window_id, event);
    }
}

fn dispatch_window_event(window_id: WindowId, event: WindowEvent) {
    use winit::event::{ElementState, MouseButton, MouseScrollDelta};
    use winit::keyboard::{Key, ModifiersState, NamedKey};

    let mut st = lock_state();
    let Some((_, surf)) = st.surfaces.iter_mut().find(|(_, s)| {
        s.window
            .as_ref()
            .map(|w| w.id() == window_id)
            .unwrap_or(false)
    }) else {
        return;
    };

    match event {
        WindowEvent::CloseRequested => {
            surf.input.close();
        }
        WindowEvent::Focused(true) => surf.input.focus(),
        WindowEvent::Focused(false) => surf.input.blur(),
        WindowEvent::CursorMoved { position, .. } => {
            surf.input
                .pointer_move(position.x as f32, position.y as f32, 0);
        }
        WindowEvent::CursorEntered { .. } => {
            let (x, y) = (surf.input.x, surf.input.y);
            surf.input.pointer_enter(x, y, 0);
        }
        WindowEvent::CursorLeft { .. } => {
            let (x, y) = (surf.input.x, surf.input.y);
            surf.input.pointer_leave(x, y, 0);
        }
        WindowEvent::MouseInput { state, button, .. } => {
            let b = match button {
                MouseButton::Left => 0,
                MouseButton::Right => 1,
                MouseButton::Middle => 2,
                MouseButton::Back => 3,
                MouseButton::Forward => 4,
                MouseButton::Other(n) => i32::from(n),
            };
            let (x, y) = (surf.input.x, surf.input.y);
            match state {
                ElementState::Pressed => surf.input.pointer_down(x, y, b, 0),
                ElementState::Released => surf.input.pointer_up(x, y, b, 0),
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let (dx, dy) = match delta {
                MouseScrollDelta::LineDelta(x, y) => (x, y),
                MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
            };
            let (x, y) = (surf.input.x, surf.input.y);
            surf.input.wheel(dx, dy, x, y);
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let code = physical_code_string(event.physical_key);
            let key = logical_key_string(&event.logical_key);
            match event.state {
                ElementState::Pressed => {
                    if !event.repeat {
                        if let Key::Character(ch) = &event.logical_key {
                            if !ch.is_empty() {
                                surf.input.text_input(ch.to_string());
                            }
                        } else if let Key::Named(NamedKey::Space) = &event.logical_key {
                            surf.input.text_input(" ".into());
                        }
                    }
                    surf.input.key_down(code, key, event.repeat);
                }
                ElementState::Released => surf.input.key_up(code, key),
            }
        }
        WindowEvent::ModifiersChanged(mods) => {
            let m: ModifiersState = mods.state();
            surf.input.shift = m.shift_key();
            surf.input.ctrl = m.control_key();
            surf.input.alt = m.alt_key();
            surf.input.meta = m.super_key();
        }
        WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
            surf.input.text_input(text);
        }
        WindowEvent::Resized(size) => {
            let w = size.width.max(1);
            let h = size.height.max(1);
            surf.width = w;
            surf.height = h;
            surf.input.resize(w as i32, h as i32);
        }
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            surf.input.scale_factor(scale_factor as f32);
        }
        WindowEvent::Touch(touch) => {
            use winit::event::TouchPhase;
            let x = touch.location.x as f32;
            let y = touch.location.y as f32;
            let pid = touch.id as i32;
            match touch.phase {
                TouchPhase::Started => surf.input.pointer_down(x, y, 0, pid),
                TouchPhase::Moved => surf.input.pointer_move(x, y, pid),
                TouchPhase::Ended => surf.input.pointer_up(x, y, 0, pid),
                TouchPhase::Cancelled => surf.input.pointer_cancel(pid),
            }
        }
        _ => {}
    }
}

fn physical_code_string(key: winit::keyboard::PhysicalKey) -> String {
    match key {
        winit::keyboard::PhysicalKey::Code(code) => format!("{code:?}"),
        winit::keyboard::PhysicalKey::Unidentified(_) => "Unidentified".into(),
    }
}

fn logical_key_string(key: &winit::keyboard::Key) -> String {
    match key {
        winit::keyboard::Key::Character(s) => s.to_string(),
        winit::keyboard::Key::Named(n) => format!("{n:?}"),
        winit::keyboard::Key::Unidentified(_) => String::new(),
        winit::keyboard::Key::Dead(c) => c.map(|ch| ch.to_string()).unwrap_or_default(),
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
/// Returns `-1` if the window or wgpu surface cannot be created (no hollow entries).
pub fn create(name: &str, width: i32, height: i32) -> i32 {
    let mut st = lock_state();
    if !st.ready {
        return -1;
    }
    let w = width.max(1) as u32;
    let h = height.max(1) as u32;

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

    let window = match created {
        Ok(Some(window)) => window,
        Ok(None) => {
            eprintln!("Dream gpuSurfaceCreate: no window (unavailable)");
            return -1;
        }
        Err(e) => {
            eprintln!("Dream gpuSurfaceCreate: {e}");
            return -1;
        }
    };

    let instance = st.instance.as_ref().unwrap();
    let surface = match instance.create_surface(window.clone()) {
        Ok(surface) => surface,
        Err(e) => {
            eprintln!("Dream gpuSurfaceCreate: surface create failed: {e}");
            return -1;
        }
    };

    if let Some(adapter) = st.adapter.as_ref() {
        let caps = surface.get_capabilities(adapter);
        // Prefer RGBA so compute `rgba8` textures can blit/copy without a format convert.
        let preferred = [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ];
        if let Some(fmt) = preferred
            .iter()
            .copied()
            .find(|f| caps.formats.contains(f))
            .or_else(|| caps.formats.first().copied())
        {
            st.render_format = fmt;
        }
    }

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
    surface.configure(&device, &config);

    let id = st.alloc_id();
    st.surfaces.insert(
        id,
        SurfaceEntry {
            width: w,
            height: h,
            color: None,
            depth: None,
            window: Some(window),
            surface: Some(surface),
            config: Some(config),
            pending_frame: None,
            input: Default::default(),
        },
    );
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
            classify_err(&e)
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
        return Err("surface not configured (validation)".into());
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
    let frame = match surface.get_current_texture() {
        Ok(f) => f,
        Err(e) => {
            if matches!(
                e,
                wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated
            ) {
                if let Some(cfg) = surf.config.clone() {
                    surface.configure(&device, &cfg);
                }
                surface.get_current_texture().map_err(|e2| {
                    format!(
                        "surface acquire failed ({})",
                        surface_err_word(&e2)
                    )
                })?
            } else {
                return Err(format!(
                    "surface acquire failed ({})",
                    surface_err_word(&e)
                ));
            }
        }
    };

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
        if let Some(e) = drain_uncaptured() {
            st.set_last_error(e.clone());
            return Err(e);
        }
    }
    frame.present();
    drop(st);

    let _ = with_event_loop(|el| {
        let mut app = PumpApp;
        let _ = el.pump_app_events(Some(Duration::ZERO), &mut app);
    });
    Ok(())
}

fn surface_err_word(err: &wgpu::SurfaceError) -> &'static str {
    match classify_surface_error(err) {
        c if c == super::state::ERR_TIMEOUT => "timeout",
        c if c == super::state::ERR_VALIDATION => "validation",
        _ => "other",
    }
}

pub fn blit(surface_id: i32, texture_id: i32) -> i32 {
    match blit_inner(surface_id, texture_id) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Dream gpuRenderBlit: {e}");
            classify_err(&e)
        }
    }
}

fn ensure_blit(
    st: &mut super::state::GpuState,
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) {
    if st.blit.as_ref().is_some_and(|b| b.format == format) {
        return;
    }
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("dream-blit-wgsl"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
struct VSOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f }
@vertex fn vs(@builtin(vertex_index) vi: u32) -> VSOut {
  var positions = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  var uvs = array<vec2f, 3>(vec2f(0.0, 1.0), vec2f(2.0, 1.0), vec2f(0.0, -1.0));
  var o: VSOut;
  o.pos = vec4f(positions[vi], 0.0, 1.0);
  o.uv = uvs[vi];
  return o;
}
@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var tex: texture_2d<f32>;
@fragment fn fs(i: VSOut) -> @location(0) vec4f {
  return textureSample(tex, samp, i.uv);
}
"#
            .into(),
        ),
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("dream-blit-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("dream-blit-pl"),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dream-blit"),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("dream-blit-samp"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    st.blit = Some(super::state::BlitPipe {
        pipeline,
        bgl,
        sampler,
        format,
    });
}

fn blit_inner(surface_id: i32, texture_id: i32) -> Result<(), String> {
    let mut st = lock_state();
    if !st.ready {
        return Err("GPU not initialized".into());
    }
    let device = st.device.as_ref().unwrap().clone();
    let queue = st.queue.as_ref().unwrap().clone();
    let format = st.render_format;
    ensure_blit(&mut st, &device, format);

    let tex = st
        .textures
        .get_mut(&texture_id)
        .ok_or_else(|| format!("unknown texture {texture_id}"))?;
    if tex.gpu.is_none() {
        let usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC;
        tex.gpu = Some(device.create_texture(&wgpu::TextureDescriptor {
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
        }));
    }
    if tex.dirty_cpu && !tex.cpu.is_empty() && !tex.depth {
        let bpp = if tex.format == wgpu::TextureFormat::Rgba16Float {
            8
        } else {
            4
        };
        let gpu = tex.gpu.as_ref().unwrap();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: gpu,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &tex.cpu,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(tex.width * bpp),
                rows_per_image: Some(tex.height),
            },
            wgpu::Extent3d {
                width: tex.width,
                height: tex.height,
                depth_or_array_layers: 1,
            },
        );
        tex.dirty_cpu = false;
    }

    let src = tex.gpu.as_ref().unwrap().clone();
    let src_view = src.create_view(&Default::default());
    let can_swapchain = {
        let surf = st
            .surfaces
            .get(&surface_id)
            .ok_or_else(|| format!("unknown surface {surface_id}"))?;
        surf.surface.is_some() && surf.config.is_some()
    };

    let bg = {
        let blit = st.blit.as_ref().unwrap();
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("dream-blit-bg"),
            layout: &blit.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&blit.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&src_view),
                },
            ],
        })
    };

    if can_swapchain {
        let surf = st.surfaces.get_mut(&surface_id).unwrap();
        surf.pending_frame = None;
        let surface = surf.surface.as_ref().unwrap();
        let frame = surface
            .get_current_texture()
            .map_err(|e| format!("surface acquire failed ({})", surface_err_word(&e)))?;
        let dst_view = frame.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("dream-blit-sc"),
        });
        {
            let blit = st.blit.as_ref().unwrap();
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dream-blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&blit.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        if let Some(e) = drain_uncaptured() {
            st.set_last_error(e.clone());
            return Err(e);
        }
        st.surfaces.get_mut(&surface_id).unwrap().pending_frame = Some(frame);
        return Ok(());
    }

    // Headless / no swapchain: blit into offscreen color matching swapchain format.
    {
        let surf = st.surfaces.get_mut(&surface_id).unwrap();
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
    }
    let dst_view = st
        .surfaces
        .get(&surface_id)
        .unwrap()
        .color
        .as_ref()
        .unwrap()
        .create_view(&Default::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("dream-blit"),
    });
    {
        let blit = st.blit.as_ref().unwrap();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dream-blit-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &dst_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&blit.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
    if let Some(e) = drain_uncaptured() {
        st.set_last_error(e.clone());
        return Err(e);
    }
    Ok(())
}

pub fn frame_tick() {
    let _ = with_event_loop(|el| {
        let mut app = PumpApp;
        // Vsync already paces present; only pump input, don't sleep.
        let _ = el.pump_app_events(Some(Duration::ZERO), &mut app);
    });
}

/// Pump the event loop then return packed pointer latch (clears dx/dy).
pub fn pointer_bytes(id: i32) -> Vec<u8> {
    let mut st = lock_state();
    st.surfaces
        .get_mut(&id)
        .map(|s| s.input.pack_pointer_and_clear_delta())
        .unwrap_or_else(|| vec![0u8; 32])
}

pub fn mods_bytes(id: i32) -> Vec<u8> {
    let st = lock_state();
    st.surfaces
        .get(&id)
        .map(|s| s.input.pack_mods())
        .unwrap_or_else(|| vec![0u8; 4])
}

pub fn key_down(id: i32, code: &str) -> bool {
    let st = lock_state();
    st.surfaces
        .get(&id)
        .map(|s| s.input.key_is_down(code))
        .unwrap_or(false)
}

pub fn focused(id: i32) -> bool {
    let st = lock_state();
    st.surfaces
        .get(&id)
        .map(|s| s.input.focused)
        .unwrap_or(false)
}

pub fn close_requested(id: i32) -> bool {
    let st = lock_state();
    st.surfaces
        .get(&id)
        .map(|s| s.input.close_requested)
        .unwrap_or(false)
}

/// Pump once, then drain the event queue (call this once per frame before reading latches).
pub fn poll_events_bytes(id: i32) -> Vec<u8> {
    frame_tick();
    let mut st = lock_state();
    st.surfaces
        .get_mut(&id)
        .map(|s| s.input.drain_events_packed())
        .unwrap_or_else(|| vec![0u8; 4])
}
