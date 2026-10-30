use crate::{
    Command, Result,
    assets::RasterImage,
    effects::{Effect, RenderStats, reserve},
    gpu::GpuSurface,
    prepare::{self, FilterBackend},
};
use std::{collections::BTreeMap, sync::Arc};
use vello_common::{TextureId, geometry::RectU16, paint::ImageSource};
use vello_gpu::TextureBindings;

pub struct Scratch {
    texture: wgpu::Texture,
    parameters: wgpu::Buffer,
}

#[derive(Default)]
pub struct Pool {
    targets: Vec<Scratch>,
    pub stats: RenderStats,
}

pub struct Frame<'a> {
    pub surface: &'a mut GpuSurface,
    pub bindings: TextureBindings,
    pub sources: BTreeMap<usize, ImageSource>,
    used: usize,
    // Texture IDs derive from these identities; keep them alive until encoding finishes.
    images: Vec<Arc<RasterImage>>,
}
impl<'a> Frame<'a> {
    pub fn new(
        surface: &'a mut GpuSurface,
        bindings: TextureBindings,
        sources: BTreeMap<usize, ImageSource>,
    ) -> Self {
        Self {
            surface,
            bindings,
            sources,
            used: 0,
            images: Vec::new(),
        }
    }
    fn acquire(&mut self) -> Result<usize> {
        let index = self.used;
        if index == self.surface.scratch.targets.len() {
            reserve(
                &mut self.surface.scratch.stats,
                self.surface.width,
                self.surface.height,
            )?;
            let texture = self
                .surface
                .device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("Dream pooled filter target"),
                    size: self.surface.target.size(),
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                });
            let parameters = self.surface.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Dream color effect parameters"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.surface.scratch.targets.push(Scratch {
                texture,
                parameters,
            });
        }
        self.used += 1;
        Ok(index)
    }
    fn bind(&mut self, index: usize) -> Arc<RasterImage> {
        let image = prepare::external_image();
        let key = Arc::as_ptr(&image) as usize;
        let id = TextureId(key as u64);
        self.bindings.insert(
            id,
            self.surface.scratch.targets[index]
                .texture
                .create_view(&Default::default()),
        );
        self.sources.insert(
            key,
            ImageSource::external_texture(
                id,
                RectU16::new(0, 0, self.surface.width, self.surface.height),
                true,
            ),
        );
        self.images.push(image.clone());
        image
    }
}
impl FilterBackend for Frame<'_> {
    fn size(&self) -> (u16, u16) {
        (self.surface.width, self.surface.height)
    }
    fn filtered(&mut self, commands: &[Command], effects: &[Effect]) -> Result<Arc<RasterImage>> {
        let mut current = self.acquire()?;
        let texture = self.surface.scratch.targets[current].texture.clone();
        self.surface
            .render_scene(commands, &texture, &self.bindings, &self.sources, None)?;
        for effect in effects {
            let next = self.acquire()?;
            let target = self.surface.scratch.targets[next].texture.clone();
            if let Some(filter) = effect.filter() {
                let image = self.bind(current);
                let command =
                    prepare::image_command(image, self.surface.width, self.surface.height);
                self.surface.render_scene(
                    &[command],
                    &target,
                    &self.bindings,
                    &self.sources,
                    Some(filter),
                )?;
            } else {
                let commands = self.surface.color_filter.encode(
                    &self.surface.device,
                    &self.surface.queue,
                    &self.surface.scratch.targets[current].texture,
                    &target,
                    &self.surface.scratch.targets[next].parameters,
                    effect,
                );
                self.surface.queue.submit([commands]);
            }
            current = next;
        }
        Ok(self.bind(current))
    }
}
