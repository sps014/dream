#[path = "command_reader.rs"]
mod reader;
use crate::{
    Result,
    assets::{Asset, RasterImage},
    text::ShapedText,
};
use reader::{Reader, invalid};
use std::sync::Arc;
use vello_common::{
    kurbo::{Affine, BezPath, Stroke},
    peniko::{Color, Fill, Gradient},
};

#[derive(Clone)]
pub enum Brush {
    Solid(Color),
    Gradient(Gradient),
}

#[derive(Clone)]
pub enum Command {
    Clear(Color),
    Draw(Box<DrawCommand>),
    Layer {
        opacity: f32,
        commands: Vec<Command>,
    },
    Backdrop {
        prefix: Arc<[Command]>,
        clips: Vec<BezPath>,
        sigma: f32,
        tint: Color,
    },
    Image(Box<ImageCommand>),
    Text {
        layout: Arc<ShapedText>,
        color: Color,
        transform: Affine,
        clips: Vec<BezPath>,
    },
}

#[derive(Clone)]
pub struct ImageCommand {
    pub image: Arc<RasterImage>,
    pub source: vello_common::kurbo::Rect,
    pub target: vello_common::kurbo::Rect,
    pub opacity: f32,
    pub nearest: bool,
    pub transform: Affine,
    pub clips: Vec<BezPath>,
}

#[derive(Clone)]
pub struct DrawCommand {
    pub path: BezPath,
    pub brush: Brush,
    pub stroke: Option<Stroke>,
    pub fill: Fill,
    pub transform: Affine,
    pub clips: Vec<BezPath>,
}

pub struct Picture {
    pub commands: Vec<Command>,
}

#[derive(Clone, Default)]
struct State {
    transform: Affine,
    clips: Vec<BezPath>,
}

impl Picture {
    pub fn images(&self) -> std::collections::BTreeMap<usize, Arc<RasterImage>> {
        fn collect(
            commands: &[Command],
            output: &mut std::collections::BTreeMap<usize, Arc<RasterImage>>,
        ) {
            for command in commands {
                match command {
                    Command::Image(draw) => {
                        output.insert(Arc::as_ptr(&draw.image) as usize, draw.image.clone());
                    }
                    Command::Layer { commands, .. } => collect(commands, output),
                    Command::Backdrop { prefix, .. } => collect(prefix, output),
                    _ => {}
                }
            }
        }
        let mut images = std::collections::BTreeMap::new();
        collect(&self.commands, &mut images);
        images
    }
    pub fn decode(values: &[f32]) -> Result<Self> {
        Self::decode_with(values, &[])
    }

    pub fn decode_with(values: &[f32], assets: &[Asset]) -> Result<Self> {
        if values.len() > 4_000_000 {
            return Err(invalid("picture exceeds the command budget"));
        }
        let mut reader = Reader { values, offset: 0 };
        let mut state = State::default();
        let mut stack = Vec::new();
        let mut groups: Vec<(f32, Vec<Command>)> = vec![(1.0, Vec::new())];
        let mut backdrops = 0;
        while reader.offset < values.len() {
            match reader.count(11)? {
                0 => {
                    if groups.len() != 1 || !groups[0].1.is_empty() {
                        return Err(invalid("clear must be the first drawing command"));
                    }
                    groups[0].1.push(Command::Clear(reader.color()?));
                }
                opcode @ (1 | 2) => {
                    let path = reader.path()?;
                    let brush = reader.brush()?;
                    let mut stroke = None;
                    let mut fill = Fill::NonZero;
                    if opcode == 2 {
                        let width = reader.value()?;
                        if width <= 0.0 {
                            return Err(invalid("stroke width must be positive"));
                        }
                        let cap = match reader.count(2)? {
                            0 => vello_common::kurbo::Cap::Butt,
                            1 => vello_common::kurbo::Cap::Round,
                            _ => vello_common::kurbo::Cap::Square,
                        };
                        let join = match reader.count(2)? {
                            0 => vello_common::kurbo::Join::Miter,
                            1 => vello_common::kurbo::Join::Round,
                            _ => vello_common::kurbo::Join::Bevel,
                        };
                        let miter = reader.value()?;
                        if miter < 1.0 {
                            return Err(invalid("miter limit must be at least one"));
                        }
                        let offset = reader.value()? as f64;
                        let count = reader.count(256)?;
                        let mut dashes = Vec::with_capacity(count);
                        for _ in 0..count {
                            let dash = reader.value()?;
                            if dash <= 0.0 {
                                return Err(invalid("dash lengths must be positive"));
                            }
                            dashes.push(dash as f64);
                        }
                        stroke = Some(
                            Stroke::new(width as f64)
                                .with_caps(cap)
                                .with_join(join)
                                .with_miter_limit(miter as f64)
                                .with_dashes(offset, dashes),
                        );
                    } else if reader.count(1)? == 1 {
                        fill = Fill::EvenOdd;
                    }
                    groups
                        .last_mut()
                        .ok_or_else(|| invalid("missing drawing group"))?
                        .1
                        .push(Command::Draw(Box::new(DrawCommand {
                            path,
                            brush,
                            stroke,
                            fill,
                            transform: state.transform,
                            clips: state.clips.clone(),
                        })));
                }
                3 => {
                    if stack.len() >= 64 {
                        return Err(invalid("drawing state exceeds 64 levels"));
                    }
                    stack.push((state.clone(), false));
                }
                4 => {
                    let (saved, layer) =
                        stack.pop().ok_or_else(|| invalid("restore without save"))?;
                    if layer {
                        let (opacity, commands) = groups
                            .pop()
                            .ok_or_else(|| invalid("missing drawing group"))?;
                        groups
                            .last_mut()
                            .ok_or_else(|| invalid("missing drawing group"))?
                            .1
                            .push(Command::Layer { opacity, commands });
                    }
                    state = saved;
                }
                5 => {
                    let mut matrix = [0.0; 6];
                    for value in &mut matrix {
                        *value = reader.value()? as f64;
                    }
                    let next = state.transform * Affine::new(matrix);
                    if next
                        .as_coeffs()
                        .iter()
                        .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
                    {
                        return Err(invalid("transform exceeds the drawing coordinate range"));
                    }
                    state.transform = next;
                }
                6 => {
                    if state.clips.len() >= 64 {
                        return Err(invalid("too many nested drawing clips"));
                    }
                    state.clips.push(state.transform * reader.path()?);
                }
                7 => {
                    let opacity = reader.value()?;
                    if !(0.0..=1.0).contains(&opacity) || stack.len() >= 64 {
                        return Err(invalid("invalid layer opacity or layer depth"));
                    }
                    stack.push((state.clone(), true));
                    groups.push((opacity, Vec::new()));
                }
                8 => {
                    if groups.len() != 1 {
                        return Err(invalid(
                            "backdrop filters cannot be recorded inside an isolated layer",
                        ));
                    }
                    backdrops += 1;
                    if backdrops > 8 {
                        return Err(invalid("a picture supports at most eight backdrop panels"));
                    }
                    let mut clips = state.clips.clone();
                    clips.push(state.transform * reader.path()?);
                    let sigma = reader.value()?;
                    if !(0.0..=128.0).contains(&sigma) {
                        return Err(invalid(
                            "backdrop blur sigma must be in 0..128 physical pixels",
                        ));
                    }
                    let tint = reader.color()?;
                    let prefix = Arc::from(groups[0].1.clone());
                    groups[0].1.push(Command::Backdrop {
                        prefix,
                        clips,
                        sigma,
                        tint,
                    });
                }
                9 => {
                    let index = reader.count(65535)?;
                    let Some(Asset::Image(image)) = assets.get(index) else {
                        return Err(invalid("image resource is missing or has the wrong kind"));
                    };
                    let source = reader.rect()?;
                    if source.x0 < 0.0
                        || source.y0 < 0.0
                        || source.x1 > f64::from(image.pixels.width())
                        || source.y1 > f64::from(image.pixels.height())
                    {
                        return Err(invalid("source rectangle is outside the image"));
                    }
                    let target = reader.rect()?;
                    let opacity = reader.value()?;
                    if !(0.0..=1.0).contains(&opacity) {
                        return Err(invalid("image opacity must be in 0..1"));
                    }
                    let nearest = reader.count(1)? == 1;
                    groups
                        .last_mut()
                        .ok_or_else(|| invalid("missing drawing group"))?
                        .1
                        .push(Command::Image(Box::new(ImageCommand {
                            image: image.clone(),
                            source,
                            target,
                            opacity,
                            nearest,
                            transform: state.transform,
                            clips: state.clips.clone(),
                        })));
                }
                opcode @ (10 | 11) => {
                    let layout = if opcode == 10 {
                        let text = reader.string(65536)?;
                        let family = reader.string(1024)?;
                        let size = reader.value()?;
                        let weight = reader.value()?;
                        let width = reader.value()?;
                        let align = reader.count(3)? as u32;
                        crate::text::layout(&text, &family, size, weight, width, align)?
                    } else {
                        let index = reader.count(65535)?;
                        let Some(Asset::Text(layout)) = assets.get(index) else {
                            return Err(invalid(
                                "text layout resource is missing or has the wrong kind",
                            ));
                        };
                        layout.clone()
                    };
                    let origin = reader.point()?;
                    let color = reader.color()?;
                    groups
                        .last_mut()
                        .ok_or_else(|| invalid("missing drawing group"))?
                        .1
                        .push(Command::Text {
                            layout,
                            color,
                            transform: state.transform * Affine::translate((origin.x, origin.y)),
                            clips: state.clips.clone(),
                        });
                }
                _ => unreachable!(),
            }
        }
        if !stack.is_empty() {
            return Err(invalid("unbalanced canvas save/restore"));
        }
        Ok(Self {
            commands: groups.remove(0).1,
        })
    }
}
