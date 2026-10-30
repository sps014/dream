use crate::commands::{Brush, Command};
use std::{collections::BTreeMap, sync::Arc};
use vello_common::{
    filter_effects::{Filter, FilterFunction},
    kurbo::{Affine, Rect, Shape},
    paint::{Image, ImageSource},
    peniko::{BlendMode, Compose, Fill, ImageQuality, ImageSampler},
};

// Both Vello recorders deliberately expose the same drawing operations. Keep ordering,
// filter placement and clipping identical instead of maintaining two replay interpreters.
macro_rules! replay {
    ($name:ident, $context:ty, $resources:ty) => {
        pub fn $name(
            context: &mut $context,
            resources: &mut $resources,
            commands: &[Command],
            width: u16,
            height: u16,
            images: &BTreeMap<usize, ImageSource>,
        ) -> crate::Result<()> {
            let viewport = Rect::new(0.0, 0.0, width as f64, height as f64).to_path(0.1);
            for command in commands {
                context.set_transform(Affine::IDENTITY);
                context.set_paint_transform(Affine::IDENTITY);
                match command {
                    Command::Clear(color) => {
                        context.set_paint(*color);
                        context.fill_path(&viewport);
                    }
                    Command::Draw(draw) => {
                        let crate::commands::DrawCommand {
                            path,
                            brush,
                            stroke,
                            fill,
                            transform,
                            clips,
                        } = draw.as_ref();
                        context.set_fill_rule(Fill::NonZero);
                        for clip in clips {
                            context.push_clip_path(clip);
                        }
                        context.set_transform(*transform);
                        context.set_fill_rule(*fill);
                        match brush {
                            Brush::Solid(color) => context.set_paint(*color),
                            Brush::Gradient(gradient) => context.set_paint(gradient.clone()),
                        }
                        if let Some(stroke) = stroke {
                            context.set_stroke(stroke.clone());
                            context.stroke_path(path);
                        } else {
                            context.fill_path(path);
                        }
                        context.set_transform(Affine::IDENTITY);
                        for _ in clips {
                            context.pop_clip();
                        }
                    }
                    Command::Layer { opacity, commands } => {
                        context.push_opacity_layer(*opacity);
                        $name(context, resources, commands, width, height, images)?;
                        context.pop_layer();
                    }
                    Command::Backdrop {
                        prefix,
                        clips,
                        sigma,
                        tint,
                    } => {
                        context.set_fill_rule(Fill::NonZero);
                        let copy = BlendMode::new(vello_common::peniko::Mix::Normal, Compose::Copy);
                        for clip in clips {
                            context.push_layer(Some(clip), Some(copy), None, None, None);
                        }
                        // Replace the clipped backdrop, rather than source-over it a second
                        // time: translucent earlier content must retain its original alpha.
                        if *sigma > 0.0 {
                            context.push_filter_layer(Filter::from_function(
                                FilterFunction::Blur { radius: *sigma },
                            ));
                        }
                        $name(context, resources, prefix, width, height, images)?;
                        if *sigma > 0.0 {
                            context.pop_layer();
                        }
                        context.set_transform(Affine::IDENTITY);
                        context.set_paint(*tint);
                        context.fill_path(&viewport);
                        for _ in clips {
                            context.pop_layer();
                        }
                    }
                    Command::Image(draw) => {
                        context.set_fill_rule(Fill::NonZero);
                        for clip in &draw.clips {
                            context.push_clip_path(clip);
                        }
                        context.set_transform(draw.transform);
                        context.set_paint_transform(
                            Affine::translate((draw.target.x0, draw.target.y0))
                                * Affine::scale_non_uniform(
                                    draw.target.width() / draw.source.width(),
                                    draw.target.height() / draw.source.height(),
                                )
                                * Affine::translate((-draw.source.x0, -draw.source.y0)),
                        );
                        let source = images
                            .get(&(Arc::as_ptr(&draw.image) as usize))
                            .cloned()
                            .unwrap_or_else(|| ImageSource::Pixmap(draw.image.pixels.clone()));
                        context.set_paint(Image {
                            image: source,
                            sampler: ImageSampler::new().with_alpha(draw.opacity).with_quality(
                                if draw.nearest {
                                    ImageQuality::Low
                                } else {
                                    ImageQuality::Medium
                                },
                            ),
                        });
                        context.fill_rect(&draw.target);
                        context.set_transform(Affine::IDENTITY);
                        for _ in &draw.clips {
                            context.pop_clip();
                        }
                    }
                    Command::Text {
                        layout,
                        color,
                        transform,
                        clips,
                    } => {
                        context.set_fill_rule(Fill::NonZero);
                        for clip in clips {
                            context.push_clip_path(clip);
                        }
                        context.set_transform(*transform);
                        context.set_paint(*color);
                        for run in &layout.runs {
                            context
                                .glyph_run(resources, &run.font)
                                .font_size(run.size)
                                .normalized_coords(&run.coords)
                                .fill_glyphs(run.glyphs.iter().copied())
                                .map_err(|error| crate::DrawingError::Render(error.to_string()))?;
                        }
                        context.set_transform(Affine::IDENTITY);
                        for _ in clips {
                            context.pop_clip();
                        }
                    }
                }
            }
            Ok(())
        }
    };
}

replay!(cpu, vello_cpu::RenderContext, vello_cpu::Resources);
replay!(gpu, vello_gpu::Scene, vello_gpu::Resources);
