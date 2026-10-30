use crate::{DrawingError, Picture, Result, replay};
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use vello_common::{TextureId, geometry::RectU16, paint::ImageSource};
use vello_gpu::{
    ClearSettings, RenderSize, RenderTargetConfig, Renderer, Resources, Scene, TargetInit,
    TextureBindings,
};

struct DeviceOwner(wgpu::Device);

impl Drop for DeviceOwner {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

#[derive(Clone)]
pub(super) struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    owner: Arc<DeviceOwner>,
    error: Arc<Mutex<Option<String>>>,
}

impl GpuContext {
    pub async fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| DrawingError::Unavailable(e.to_string()))?;
        let requested =
            adapter.features() & (wgpu::Features::SHADER_F16 | wgpu::Features::SUBGROUP);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: requested,
                ..Default::default()
            })
            .await
            .map_err(|e| DrawingError::Unavailable(e.to_string()))?;
        let error = Arc::new(Mutex::new(None));
        let errors = error.clone();
        device.on_uncaptured_error(Arc::new(move |failure: wgpu::Error| {
            *errors.lock().unwrap_or_else(|e| e.into_inner()) = Some(failure.to_string());
        }));
        let errors = error.clone();
        device.set_device_lost_callback(move |reason, message| {
            *errors.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(format!("device lost ({reason:?}): {message}"));
        });
        let owner = Arc::new(DeviceOwner(device.clone()));
        Ok(Self {
            device,
            queue,
            error,
            owner,
        })
    }
}

pub struct GpuSurface {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) target: wgpu::Texture,
    pub(super) renderer: Renderer,
    pub(super) resources: Resources,
    pub(super) width: u16,
    pub(super) height: u16,
    error: Arc<Mutex<Option<String>>>,
    pub(super) scratch: crate::gpu_effects::Pool,
    pub(super) color_filter: crate::gpu_color::ColorFilter,
    reads: AtomicU64,
    _owner: Arc<DeviceOwner>,
    images: BTreeMap<usize, (Arc<crate::assets::RasterImage>, wgpu::Texture)>,
}

impl GpuSurface {
    pub async fn new(width: u16, height: u16) -> Result<Self> {
        Self::with_context(width, height, GpuContext::new().await?)
    }

    pub(super) fn with_context(width: u16, height: u16, context: GpuContext) -> Result<Self> {
        let GpuContext {
            device,
            queue,
            error,
            owner,
        } = context;
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Dream drawing target"),
            size: wgpu::Extent3d {
                width: width.into(),
                height: height.into(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let (renderer, resources) = Renderer::new(
            &device,
            &RenderTargetConfig {
                format,
                width,
                height,
            },
        );
        let color_filter = crate::gpu_color::ColorFilter::new(&device);
        Ok(Self {
            device,
            queue,
            target,
            renderer,
            resources,
            width,
            height,
            error,
            images: BTreeMap::new(),
            scratch: Default::default(),
            color_filter,
            reads: AtomicU64::new(0),
            _owner: owner,
        })
    }

    pub fn render(&mut self, picture: &Picture) -> Result<()> {
        self.check()?;
        let active = picture.images();
        self.images.retain(|id, _| active.contains_key(id));
        let mut bindings = TextureBindings::new();
        let mut sources = BTreeMap::new();
        for (key, image) in active {
            let (_, texture) = self.images.entry(key).or_insert_with(|| {
                let size = wgpu::Extent3d {
                    width: u32::from(image.pixels.width()),
                    height: u32::from(image.pixels.height()),
                    depth_or_array_layers: 1,
                };
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Dream drawing image"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
                self.queue.write_texture(
                    texture.as_image_copy(),
                    image.pixels.data_as_u8_slice(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size.width * 4),
                        rows_per_image: None,
                    },
                    size,
                );
                (image.clone(), texture)
            });
            let id = TextureId(key as u64);
            bindings.insert(id, texture.create_view(&Default::default()));
            sources.insert(
                key,
                ImageSource::external_texture(
                    id,
                    RectU16::new(0, 0, image.pixels.width(), image.pixels.height()),
                    image.pixels.may_have_transparency(),
                ),
            );
        }
        let mut frame = crate::gpu_effects::Frame::new(self, bindings, sources);
        let commands = crate::prepare::commands(&mut frame, &picture.commands)?;
        let target = frame.surface.target.clone();
        frame
            .surface
            .render_scene(&commands, &target, &frame.bindings, &frame.sources, None)?;
        self.check()
    }

    pub(super) fn render_scene(
        &mut self,
        commands: &[crate::Command],
        target: &wgpu::Texture,
        bindings: &TextureBindings,
        sources: &BTreeMap<usize, ImageSource>,
        filter: Option<vello_common::filter_effects::Filter>,
    ) -> Result<()> {
        let mut scene = Scene::new(self.width, self.height);
        let filtered = filter.is_some();
        if let Some(filter) = filter {
            scene.push_filter_layer(filter);
        }
        replay::gpu(
            &mut scene,
            &mut self.resources,
            commands,
            self.width,
            self.height,
            sources,
        )?;
        if filtered {
            scene.pop_layer();
        }
        let view = target.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.renderer
            .render(
                &scene,
                &mut self.resources,
                &self.device,
                &self.queue,
                &mut encoder,
                &RenderSize {
                    width: self.width,
                    height: self.height,
                },
                &view,
                None,
                bindings,
                TargetInit::Clear(ClearSettings::default()),
            )
            .map_err(|e| DrawingError::Render(e.to_string()))?;
        self.queue.submit([encoder.finish()]);
        self.check()
    }

    pub fn stats(&self) -> crate::RenderStats {
        let mut stats = self.scratch.stats;
        stats.pixel_reads = self.reads.load(Ordering::Relaxed);
        stats
    }

    pub async fn pixels(&self) -> Result<Vec<u8>> {
        self.check()?;
        self.reads.fetch_add(1, Ordering::Relaxed);
        let row_bytes = u32::from(self.width) * 4;
        let padded = row_bytes.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dream drawing pixel read"),
            size: u64::from(padded) * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: None,
                },
            },
            self.target.size(),
        );
        self.queue.submit([encoder.finish()]);
        let (sender, receiver) = futures_channel::oneshot::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        #[cfg(not(target_arch = "wasm32"))]
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| DrawingError::Render(e.to_string()))?;
        receiver
            .await
            .map_err(|e| DrawingError::Render(e.to_string()))?
            .map_err(|e| DrawingError::Render(e.to_string()))?;
        let mapped = buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|e| DrawingError::Render(e.to_string()))?;
        let mut pixels = Vec::with_capacity(row_bytes as usize * usize::from(self.height));
        for row in mapped.chunks_exact(padded as usize) {
            pixels.extend_from_slice(&row[..row_bytes as usize]);
        }
        drop(mapped);
        buffer.unmap();
        self.check()?;
        Ok(pixels)
    }

    fn check(&self) -> Result<()> {
        match &*self.error.lock().unwrap_or_else(|e| e.into_inner()) {
            Some(error) => Err(DrawingError::Render(error.clone())),
            None => Ok(()),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a real GPU; absence is a failure"]
    fn shared_device_survives_releasing_one_surface() {
        let context = pollster::block_on(GpuContext::new()).unwrap();
        let first = GpuSurface::with_context(8, 8, context.clone()).unwrap();
        let mut second = GpuSurface::with_context(8, 8, context.clone()).unwrap();
        drop(context);
        drop(first);
        second
            .render(&Picture::decode(&[0., 0.25, 0.5, 0.75, 1.]).unwrap())
            .unwrap();
        let pixels = pollster::block_on(second.pixels()).unwrap();
        assert_eq!(&pixels[..4], &[64, 128, 191, 255]);
    }
}
