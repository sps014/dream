//! Resolving a decoded `PassDesc` into concrete wgpu attachment views.

use super::super::error::classify_surface_error;
use super::super::state::GpuState;
use super::decode::PassDesc;

pub const TARGET_SURFACE: i32 = 0;
pub const TARGET_TEXTURE: i32 = 1;
/// `_depth_id` sentinel asking for the surface-managed depth texture.
const DEPTH_FROM_SURFACE: i32 = -2;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

pub struct ColorView {
    pub view: wgpu::TextureView,
    pub resolve: Option<wgpu::TextureView>,
    pub load: wgpu::LoadOp<wgpu::Color>,
    pub store: wgpu::StoreOp,
}

pub struct DepthView {
    pub view: wgpu::TextureView,
    pub load: wgpu::LoadOp<f32>,
    pub store: wgpu::StoreOp,
    /// `None` unless the attached format actually carries a stencil aspect; wgpu rejects stencil
    /// ops on a stencil-less format.
    pub stencil: Option<(wgpu::LoadOp<u32>, wgpu::StoreOp)>,
}

pub struct Attachments {
    pub colors: Vec<ColorView>,
    pub depth: Option<DepthView>,
    /// Format of color target 0; selects the pipeline variant.
    pub format: wgpu::TextureFormat,
    /// Texture ids written by this pass, so their CPU mirrors and blit caches can be invalidated.
    pub textures: Vec<i32>,
}

/// Acquires (or reuses) the swapchain frame for `surface_id`.
///
/// The frame is parked in `pending_frame` rather than returned so several passes in one submit,
/// and `present` afterwards, all draw into the same acquired texture.
fn ensure_frame(
    st: &mut GpuState,
    device: &wgpu::Device,
    surface_id: i32,
) -> Result<bool, String> {
    let surf = st
        .surfaces
        .get_mut(&surface_id)
        .ok_or_else(|| format!("unknown surface {surface_id}"))?;
    if surf.surface.is_none() || surf.config.is_none() {
        return Ok(false);
    }
    if surf.pending_frame.is_some() {
        return Ok(true);
    }
    super::super::profile::note_size(surf.width, surf.height);
    let acquire = super::super::profile::Span::start();
    let surface = surf.surface.as_ref().unwrap();
    let frame = match surface.get_current_texture() {
        Ok(f) => f,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            if let Some(cfg) = surf.config.clone() {
                surf.surface.as_ref().unwrap().configure(device, &cfg);
            }
            surf.surface
                .as_ref()
                .unwrap()
                .get_current_texture()
                .map_err(|e2| acquire_error(&e2))?
        }
        Err(e) => return Err(acquire_error(&e)),
    };
    if let Some(s) = acquire {
        super::super::profile::note_acquire(s.elapsed());
    }
    surf.pending_frame = Some(frame);
    Ok(true)
}

fn acquire_error(e: &wgpu::SurfaceError) -> String {
    let kind = match classify_surface_error(e) {
        c if c == super::super::state::ERR_TIMEOUT => "timeout",
        c if c == super::super::state::ERR_VALIDATION => "validation",
        _ => "other",
    };
    format!("surface acquire failed ({kind})")
}

/// Offscreen mirror of a surface, used when there is no window swapchain (headless / blit path).
fn surface_color(
    st: &mut GpuState,
    device: &wgpu::Device,
    surface_id: i32,
    format: wgpu::TextureFormat,
) -> Result<wgpu::TextureView, String> {
    let surf = st
        .surfaces
        .get_mut(&surface_id)
        .ok_or_else(|| format!("unknown surface {surface_id}"))?;
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
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        }));
    }
    Ok(surf
        .color
        .as_ref()
        .unwrap()
        .create_view(&Default::default()))
}

/// Multisampled color target for a surface pass; resolved into the swapchain view.
fn surface_msaa(
    st: &mut GpuState,
    device: &wgpu::Device,
    surface_id: i32,
    format: wgpu::TextureFormat,
    samples: u32,
) -> Result<wgpu::TextureView, String> {
    let surf = st
        .surfaces
        .get_mut(&surface_id)
        .ok_or_else(|| format!("unknown surface {surface_id}"))?;
    if surf.msaa.is_none() || surf.msaa_samples != samples {
        surf.msaa = Some(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dream-surface-msaa"),
            size: wgpu::Extent3d {
                width: surf.width.max(1),
                height: surf.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
        surf.msaa_samples = samples;
    }
    Ok(surf.msaa.as_ref().unwrap().create_view(&Default::default()))
}

fn surface_depth(
    st: &mut GpuState,
    device: &wgpu::Device,
    surface_id: i32,
    samples: u32,
) -> Result<wgpu::TextureView, String> {
    let surf = st
        .surfaces
        .get_mut(&surface_id)
        .ok_or_else(|| format!("unknown surface {surface_id}"))?;
    if surf.depth.is_none() || surf.depth_samples != samples {
        surf.depth = Some(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dream-surface-depth"),
            size: wgpu::Extent3d {
                width: surf.width.max(1),
                height: surf.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
        surf.depth_samples = samples;
    }
    Ok(surf
        .depth
        .as_ref()
        .unwrap()
        .create_view(&Default::default()))
}

/// Ensures a `GpuTexture` is usable as a render attachment and returns its view + format.
fn texture_attachment(
    st: &mut GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: i32,
    samples: u32,
) -> Result<(wgpu::TextureView, wgpu::TextureFormat), String> {
    if samples > 1 {
        return Err(format!(
            "texture {id} cannot be an MSAA attachment: offscreen multisampling needs a multisampled texture"
        ));
    }
    let depth = st
        .textures
        .get(&id)
        .map(|t| t.depth)
        .ok_or_else(|| format!("unknown texture {id}"))?;
    if depth {
        let t = st.textures.get_mut(&id).unwrap();
        if t.gpu.is_none() {
            t.gpu = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("dream-depth"),
                size: wgpu::Extent3d {
                    width: t.width.max(1),
                    height: t.height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: t.mip_levels.max(1),
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }));
        }
    } else {
        super::super::compute::ensure_texture(st, device, queue, id, false)?;
    }
    let t = st
        .textures
        .get(&id)
        .ok_or_else(|| format!("unknown texture {id}"))?;
    let gpu = t
        .gpu
        .as_ref()
        .ok_or_else(|| format!("texture {id} not on GPU"))?;
    // Depth textures ignore `TexEntry::format`: the GPU texture is always created as `DEPTH_FORMAT`.
    let format = if depth { DEPTH_FORMAT } else { t.format };
    Ok((super::super::textures::default_view(t, gpu), format))
}

fn load_color(op: i32, clear: [f32; 4]) -> wgpu::LoadOp<wgpu::Color> {
    if op == 1 {
        wgpu::LoadOp::Load
    } else {
        wgpu::LoadOp::Clear(wgpu::Color {
            r: clear[0] as f64,
            g: clear[1] as f64,
            b: clear[2] as f64,
            a: clear[3] as f64,
        })
    }
}

fn store(op: i32) -> wgpu::StoreOp {
    if op == 1 {
        wgpu::StoreOp::Discard
    } else {
        wgpu::StoreOp::Store
    }
}

/// Resolves every attachment the pass declares. `sample_count` comes from the pipeline, since the
/// attachment sample count and the pipeline's must agree.
pub fn resolve(
    st: &mut GpuState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    desc: &PassDesc,
    sample_count: u32,
) -> Result<Attachments, String> {
    if desc.colors.is_empty() {
        return Err("render pass has no color attachments".into());
    }
    let mut colors = Vec::with_capacity(desc.colors.len());
    let mut format = None;
    let mut textures = Vec::new();

    for c in &desc.colors {
        let (view, resolve, fmt) = match c.kind {
            TARGET_SURFACE => {
                let fmt = st.render_format;
                let swap = ensure_frame(st, device, c.id)?;
                // With a swapchain, draw (or resolve) straight into the acquired frame; without
                // one, into the offscreen mirror that `present` blits.
                let single = if swap {
                    st.surfaces
                        .get(&c.id)
                        .and_then(|s| s.pending_frame.as_ref())
                        .map(|f| f.texture.create_view(&Default::default()))
                        .ok_or_else(|| format!("surface {} has no acquired frame", c.id))?
                } else {
                    surface_color(st, device, c.id, fmt)?
                };
                if sample_count > 1 {
                    let msaa = surface_msaa(st, device, c.id, fmt, sample_count)?;
                    (msaa, Some(single), fmt)
                } else {
                    (single, None, fmt)
                }
            }
            TARGET_TEXTURE => {
                let (view, fmt) = texture_attachment(st, device, queue, c.id, sample_count)?;
                textures.push(c.id);
                let resolve = if c.resolve_id >= 0 {
                    textures.push(c.resolve_id);
                    Some(texture_attachment(st, device, queue, c.resolve_id, 1)?.0)
                } else {
                    None
                };
                (view, resolve, fmt)
            }
            other => return Err(format!("unknown color attachment target {other}")),
        };
        if format.is_none() {
            format = Some(fmt);
        }
        colors.push(ColorView {
            view,
            resolve,
            load: load_color(c.load, c.clear),
            store: store(c.store),
        });
    }

    let depth_view = if desc.depth_id == DEPTH_FROM_SURFACE {
        let surface_id = desc
            .colors
            .iter()
            .find(|c| c.kind == TARGET_SURFACE)
            .map(|c| c.id)
            .ok_or("surface_depth() needs a surface color attachment")?;
        Some((
            surface_depth(st, device, surface_id, sample_count)?,
            DEPTH_FORMAT,
        ))
    } else if desc.depth_id >= 0 {
        Some(texture_attachment(
            st,
            device,
            queue,
            desc.depth_id,
            sample_count,
        )?)
    } else if desc.depth_id != -1 {
        return Err(format!("unknown depth attachment target {}", desc.depth_id));
    } else {
        None
    };
    let depth = depth_view.map(|(view, fmt)| DepthView {
        view,
        load: if desc.depth_load == 1 {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(desc.depth_clear)
        },
        store: store(desc.depth_store),
        stencil: fmt.has_stencil_aspect().then(|| {
            (
                if desc.stencil_load == 1 {
                    wgpu::LoadOp::Load
                } else {
                    wgpu::LoadOp::Clear(desc.stencil_clear.max(0) as u32)
                },
                store(desc.stencil_store),
            )
        }),
    });

    Ok(Attachments {
        colors,
        depth,
        format: format.unwrap(),
        textures,
    })
}