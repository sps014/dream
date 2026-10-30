use crate::{
    Command, Picture, Result,
    assets::RasterImage,
    effects::{Effect, RenderStats, reserve},
    prepare::{self, FilterBackend},
    replay,
};
use std::sync::Arc;
use vello_cpu::{Pixmap, RenderContext, RenderSettings, Resources};

pub struct CpuSurface {
    width: u16,
    height: u16,
    pub pixels: Pixmap,
    resources: Resources,
    pool: Vec<Pixmap>,
    stats: RenderStats,
}

impl CpuSurface {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            pixels: Pixmap::new(width, height),
            resources: Resources::new(),
            pool: Vec::new(),
            stats: RenderStats::default(),
        }
    }
    pub fn stats(&self) -> RenderStats {
        self.stats
    }
    pub fn render(&mut self, picture: &Picture) -> Result<()> {
        let mut frame = Frame {
            surface: self,
            images: Vec::new(),
        };
        let result = (|| {
            let commands = prepare::commands(&mut frame, &picture.commands)?;
            let mut output = Pixmap::new(frame.surface.width, frame.surface.height);
            render(&mut output, &mut frame.surface.resources, &commands, None)?;
            frame.surface.pixels = output;
            Ok(())
        })();
        // Prepared command images have been dropped; their storage is reusable next frame.
        for image in frame.images {
            if let Ok(image) = Arc::try_unwrap(image)
                && let Ok(pixels) = Arc::try_unwrap(image.pixels)
            {
                frame.surface.pool.push(pixels);
            }
        }
        frame.surface.stats.scratch_bytes = frame.surface.pool.len()
            * usize::from(frame.surface.width)
            * usize::from(frame.surface.height)
            * 4;
        result
    }
}

fn render(
    output: &mut Pixmap,
    resources: &mut Resources,
    commands: &[Command],
    filter: Option<vello_common::filter_effects::Filter>,
) -> Result<()> {
    output.data_as_u8_slice_mut().fill(0);
    let settings = RenderSettings::default();
    #[cfg(not(target_arch = "wasm32"))]
    let settings = RenderSettings {
        num_threads: if filter.is_some() || prepare::uses_filters(commands) {
            0
        } else {
            std::thread::available_parallelism()
                .map_or(0, |n| n.get().saturating_sub(1).min(8) as u16)
        },
        ..settings
    };
    let mut context = RenderContext::new_with(output.width(), output.height(), settings);
    let filtered = filter.is_some();
    if let Some(filter) = filter {
        context.push_filter_layer(filter);
    }
    replay::cpu(
        &mut context,
        resources,
        commands,
        output.width(),
        output.height(),
        &Default::default(),
    )?;
    if filtered {
        context.pop_layer();
    }
    context.flush();
    context.render(&mut *output, resources);
    output.recompute_may_have_transparency();
    Ok(())
}

struct Frame<'a> {
    surface: &'a mut CpuSurface,
    images: Vec<Arc<RasterImage>>,
}
impl Frame<'_> {
    fn acquire(&mut self) -> Result<Pixmap> {
        if let Some(pixels) = self.surface.pool.pop() {
            return Ok(pixels);
        }
        reserve(
            &mut self.surface.stats,
            self.surface.width,
            self.surface.height,
        )?;
        Ok(Pixmap::new(self.surface.width, self.surface.height))
    }
}
impl FilterBackend for Frame<'_> {
    fn size(&self) -> (u16, u16) {
        (self.surface.width, self.surface.height)
    }
    fn filtered(&mut self, commands: &[Command], effects: &[Effect]) -> Result<Arc<RasterImage>> {
        let mut input = self.acquire()?;
        render(&mut input, &mut self.surface.resources, commands, None)?;
        for effect in effects {
            if let Some(filter) = effect.filter() {
                let image = Arc::new(RasterImage {
                    pixels: Arc::new(input),
                });
                let command =
                    prepare::image_command(image.clone(), self.surface.width, self.surface.height);
                let mut output = self.acquire()?;
                render(
                    &mut output,
                    &mut self.surface.resources,
                    &[command],
                    Some(filter),
                )?;
                let image = Arc::try_unwrap(image).map_err(|_| {
                    crate::DrawingError::Render("filter input still referenced".into())
                })?;
                let pixels = Arc::try_unwrap(image.pixels).map_err(|_| {
                    crate::DrawingError::Render("filter pixels still referenced".into())
                })?;
                self.surface.pool.push(pixels);
                input = output;
            } else {
                for pixel in input.data_mut() {
                    effect.adjust(pixel);
                }
            }
        }
        input.recompute_may_have_transparency();
        let image = Arc::new(RasterImage {
            pixels: Arc::new(input),
        });
        self.images.push(image.clone());
        Ok(image)
    }
}
