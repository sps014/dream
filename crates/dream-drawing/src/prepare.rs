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
    /// Returns a snapshot from `filtered` whose rendering has been submitted and that no
    /// prepared command references any more.
    fn recycle(&mut self, image: Arc<RasterImage>);
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

#[cfg(test)]
thread_local! {
    pub static UNBATCHED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn flatten_backdrops() -> bool {
    #[cfg(test)]
    if UNBATCHED.get() {
        return false;
    }
    true
}

/// Resolves layer effects to images and gives each backdrop its prefix. A backdrop replays
/// everything before it in its group, so a backdrop after another would replay that one's prefix
/// too and cost would double per panel. Once a group holds a backdrop, the next backdrop first
/// flattens the group so far into one snapshot image, keeping the work linear in the panels.
pub fn commands(backend: &mut impl FilterBackend, input: &[Command]) -> Result<Vec<Command>> {
    let mut output: Vec<Command> = Vec::with_capacity(input.len());
    let mut snapshot: Option<Arc<RasterImage>> = None;
    for command in input {
        match command {
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
                output.push(Command::Layer {
                    opacity: *opacity,
                    blend: *blend,
                    effects: Vec::new(),
                    clips: clips.clone(),
                    commands: children,
                });
            }
            Command::Backdrop {
                clips,
                sigma,
                tint,
                noise,
                ..
            } => {
                if flatten_backdrops()
                    && output
                        .iter()
                        .any(|command| matches!(command, Command::Backdrop { .. }))
                {
                    let image = backend.filtered(&output, &[])?;
                    let (width, height) = backend.size();
                    output = vec![image_command(image.clone(), width, height)];
                    if let Some(previous) = snapshot.replace(image) {
                        backend.recycle(previous);
                    }
                }
                output.push(Command::Backdrop {
                    prefix: Arc::from(output.clone()),
                    clips: clips.clone(),
                    sigma: *sigma,
                    tint: *tint,
                    noise: noise.clone(),
                });
            }
            _ => output.push(command.clone()),
        }
    }
    Ok(output)
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
