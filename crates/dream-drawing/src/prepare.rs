use crate::{
    Result,
    assets::RasterImage,
    commands::{Command, ImageCommand},
    effects::Effect,
};
use std::sync::Arc;
use vello_common::{
    kurbo::{Affine, Rect},
    pixmap::Pixmap,
};

pub trait FilterBackend {
    fn filtered(&mut self, commands: &[Command], effects: &[Effect]) -> Result<Arc<RasterImage>>;
    fn size(&self) -> (u16, u16);
}

pub fn image_command(image: Arc<RasterImage>, width: u16, height: u16) -> Command {
    let rect = Rect::new(0., 0., f64::from(width), f64::from(height));
    Command::Image(Box::new(ImageCommand {
        image,
        source: rect,
        target: rect,
        opacity: 1.,
        nearest: true,
        transform: Affine::IDENTITY,
        clips: Vec::new(),
    }))
}

// An external GPU image needs an identity and lifetime, but never CPU pixel storage.
pub fn external_image() -> Arc<RasterImage> {
    Arc::new(RasterImage {
        pixels: Arc::new(Pixmap::new(1, 1)),
    })
}

pub fn commands(backend: &mut impl FilterBackend, input: &[Command]) -> Result<Vec<Command>> {
    input
        .iter()
        .map(|command| match command {
            Command::Layer {
                opacity,
                blend,
                effects,
                clips,
                commands: children,
            } => {
                let children = commands(backend, children)?;
                let children = if effects.is_empty() {
                    children
                } else {
                    let image = backend.filtered(&children, effects)?;
                    let (width, height) = backend.size();
                    vec![image_command(image, width, height)]
                };
                Ok(Command::Layer {
                    opacity: *opacity,
                    blend: *blend,
                    effects: Vec::new(),
                    clips: clips.clone(),
                    commands: children,
                })
            }
            Command::Backdrop {
                prefix,
                clips,
                sigma,
                tint,
                noise,
            } => Ok(Command::Backdrop {
                prefix: Arc::from(commands(backend, prefix)?),
                clips: clips.clone(),
                sigma: *sigma,
                tint: *tint,
                noise: noise.clone(),
            }),
            _ => Ok(command.clone()),
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn uses_filters(commands: &[Command]) -> bool {
    commands.iter().any(|command| match command {
        Command::Backdrop { .. } => true,
        Command::Layer {
            effects, commands, ..
        } => !effects.is_empty() || uses_filters(commands),
        _ => false,
    })
}
