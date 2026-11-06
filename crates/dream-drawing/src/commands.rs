#[path = "command_reader.rs"]
mod reader;
use crate::{
    Result,
    assets::{Asset, RasterImage},
    effects::Effect,
    text::ShapedText,
};
use reader::{Reader, invalid};
use std::sync::Arc;
use vello_common::{
    kurbo::{Affine, BezPath, Shape, Stroke},
    peniko::{BlendMode, Color, Fill, Gradient},
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
        blend: BlendMode,
        effects: Vec<Effect>,
        clips: Vec<BezPath>,
        commands: Vec<Command>,
    },
    Backdrop {
        /// Earlier drawing in the same group, filled by `prepare` just before rendering.
        prefix: Arc<[Command]>,
        clips: Vec<BezPath>,
        sigma: f32,
        tint: Color,
        noise: Option<crate::noise::Noise>,
    },
    Image(Box<ImageCommand>),
    BoxShadow {
        rect: vello_common::kurbo::Rect,
        radius: f32,
        sigma: f32,
        color: Color,
        transform: Affine,
        clips: Vec<BezPath>,
    },
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

#[derive(Default)]
struct Group {
    opacity: f32,
    blend: BlendMode,
    effects: Vec<Effect>,
    clips: Vec<BezPath>,
    commands: Vec<Command>,
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
                    Command::Backdrop {
                        noise: Some(noise), ..
                    } => {
                        output.insert(Arc::as_ptr(&noise.image) as usize, noise.image.clone());
                    }
                    _ => {}
                }
            }
        }
        let mut images = std::collections::BTreeMap::new();
        collect(&self.commands, &mut images);
        images
    }
    pub fn decode(values: &[f32]) -> Result<Self> {
        Self::decode_with(values, &[], "")
    }

    /// Decodes a canvas recording. `text` is the canvas text pool that text commands index
    /// in UTF-16 code units.
    pub fn decode_with(values: &[f32], assets: &[Asset], text: &str) -> Result<Self> {
        if values.len() > 4_000_000 {
            return Err(invalid("picture exceeds the command budget"));
        }
        let mut decoder = Decoder {
            pool: text.encode_utf16().collect(),
            state: State::default(),
            stack: Vec::new(),
            groups: vec![Group {
                opacity: 1.,
                ..Default::default()
            }],
            backdrops: 0,
        };
        decoder.run(values, assets, 0, None, 0)?;
        Ok(Self {
            commands: decoder.groups.remove(0).commands,
        })
    }
}

struct Decoder {
    pool: Vec<u16>,
    state: State,
    stack: Vec<(State, bool)>,
    groups: Vec<Group>,
    backdrops: usize,
}

impl Decoder {
    fn group(&mut self) -> Result<&mut Group> {
        self.groups
            .last_mut()
            .ok_or_else(|| invalid("missing drawing group"))
    }

    fn text(&self, reader: &mut Reader, base: usize, limit: usize) -> Result<String> {
        let offset = reader.count(16_777_216)?;
        let length = reader.count(limit)?;
        let start = base + offset;
        let units = self
            .pool
            .get(start..start + length)
            .ok_or_else(|| invalid("text reference is outside the canvas text pool"))?;
        Ok(String::from_utf16_lossy(units))
    }

    /// Decodes one recording. A nested picture (`bounds` set) shares the enclosing groups so
    /// its backdrops see earlier drawing, and its resource and text indices start at the
    /// supplied bases.
    fn run(
        &mut self,
        values: &[f32],
        assets: &[Asset],
        text_base: usize,
        bounds: Option<(f64, f64)>,
        depth: usize,
    ) -> Result<()> {
        let mut reader = Reader { values, offset: 0 };
        let entry = self.stack.len();
        while reader.offset < values.len() {
            let first = reader.offset == 0;
            match reader.count(13)? {
                0 => {
                    let color = reader.color()?;
                    if !first
                        || (bounds.is_none()
                            && (self.groups.len() != 1 || !self.groups[0].commands.is_empty()))
                    {
                        return Err(invalid("clear must be the first drawing command"));
                    }
                    let command = match bounds {
                        None => Command::Clear(color),
                        Some((width, height)) => Command::Draw(Box::new(DrawCommand {
                            path: vello_common::kurbo::Rect::new(0.0, 0.0, width, height)
                                .to_path(0.1),
                            brush: Brush::Solid(color),
                            stroke: None,
                            fill: Fill::NonZero,
                            transform: self.state.transform,
                            clips: self.state.clips.clone(),
                        })),
                    };
                    self.group()?.commands.push(command);
                }
                opcode @ (1 | 2) => {
                    let path = reader.path()?;
                    let brush = reader.brush()?;
                    let mut stroke = None;
                    let mut fill = Fill::NonZero;
                    if opcode == 2 {
                        stroke = Some(reader.stroke()?);
                    } else if reader.count(1)? == 1 {
                        fill = Fill::EvenOdd;
                    }
                    let command = Command::Draw(Box::new(DrawCommand {
                        path,
                        brush,
                        stroke,
                        fill,
                        transform: self.state.transform,
                        clips: self.state.clips.clone(),
                    }));
                    self.group()?.commands.push(command);
                }
                3 => {
                    if self.stack.len() >= 64 {
                        return Err(invalid("drawing state exceeds 64 levels"));
                    }
                    self.stack.push((self.state.clone(), false));
                }
                4 => {
                    if self.stack.len() <= entry {
                        return Err(invalid("restore without save"));
                    }
                    let (saved, layer) = self
                        .stack
                        .pop()
                        .ok_or_else(|| invalid("restore without save"))?;
                    if layer {
                        let group = self
                            .groups
                            .pop()
                            .ok_or_else(|| invalid("missing drawing group"))?;
                        self.group()?.commands.push(Command::Layer {
                            opacity: group.opacity,
                            blend: group.blend,
                            effects: group.effects,
                            clips: group.clips,
                            commands: group.commands,
                        });
                    }
                    self.state = saved;
                }
                5 => {
                    let mut matrix = [0.0; 6];
                    for value in &mut matrix {
                        *value = reader.value()? as f64;
                    }
                    let next = self.state.transform * Affine::new(matrix);
                    if next
                        .as_coeffs()
                        .iter()
                        .any(|v| !v.is_finite() || v.abs() > 1_000_000.0)
                    {
                        return Err(invalid("transform exceeds the drawing coordinate range"));
                    }
                    self.state.transform = next;
                }
                6 => {
                    if self.state.clips.len() >= 64 {
                        return Err(invalid("too many nested drawing clips"));
                    }
                    let clip = self.state.transform * reader.path()?;
                    self.state.clips.push(clip);
                }
                7 => {
                    let opacity = reader.value()?;
                    if !(0.0..=1.0).contains(&opacity) || self.stack.len() >= 64 {
                        return Err(invalid("invalid layer opacity or layer depth"));
                    }
                    self.stack.push((self.state.clone(), true));
                    let group = Group {
                        opacity,
                        blend: reader.blend()?,
                        effects: reader.effects()?,
                        clips: self.state.clips.clone(),
                        commands: Vec::new(),
                    };
                    self.groups.push(group);
                    // Inherited clips bound the composite, not both the source and its edges.
                    self.state.clips.clear();
                }
                8 => {
                    self.backdrops += 1;
                    if self.backdrops > crate::MAX_BACKDROPS {
                        return Err(invalid(&format!(
                            "a picture supports at most {} backdrop panels",
                            crate::MAX_BACKDROPS
                        )));
                    }
                    let mut clips = self.state.clips.clone();
                    clips.push(self.state.transform * reader.path()?);
                    let sigma = reader.value()?;
                    if !(0.0..=128.0).contains(&sigma) {
                        return Err(invalid(
                            "backdrop blur sigma must be in 0..128 physical pixels",
                        ));
                    }
                    let tint = reader.color()?;
                    let opacity = reader.value()?;
                    if !(0.0..=1.0).contains(&opacity) {
                        return Err(invalid("backdrop noise opacity must be in 0..1"));
                    }
                    let low = reader.count(65535)? as u32;
                    let high = reader.count(65535)? as u32;
                    let noise = if opacity == 0. {
                        None
                    } else {
                        Some(crate::noise::Noise {
                            opacity,
                            image: crate::noise::tile(low | (high << 16)),
                        })
                    };
                    self.group()?.commands.push(Command::Backdrop {
                        prefix: Arc::from([]),
                        clips,
                        sigma,
                        tint,
                        noise,
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
                    let command = Command::Image(Box::new(ImageCommand {
                        image: image.clone(),
                        source,
                        target,
                        opacity,
                        nearest,
                        transform: self.state.transform,
                        clips: self.state.clips.clone(),
                    }));
                    self.group()?.commands.push(command);
                }
                opcode @ (10 | 11) => {
                    let layout = if opcode == 10 {
                        let text = self.text(&mut reader, text_base, 65536)?;
                        let family = self.text(&mut reader, text_base, 1024)?;
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
                    let command = Command::Text {
                        layout,
                        color,
                        transform: self.state.transform * Affine::translate((origin.x, origin.y)),
                        clips: self.state.clips.clone(),
                    };
                    self.group()?.commands.push(command);
                }
                13 => {
                    let origin = reader.point()?;
                    let size = reader.point()?;
                    let radius = reader.value()?;
                    let offset = reader.point()?;
                    let sigma = reader.value()?;
                    let spread = f64::from(reader.value()?);
                    let color = reader.color()?;
                    if size.x < 0.0
                        || size.y < 0.0
                        || radius < 0.0
                        || !(0.0..=128.0).contains(&sigma)
                    {
                        return Err(invalid(
                            "box shadow needs a non-negative size and radius and sigma in 0..128",
                        ));
                    }
                    let rect = vello_common::kurbo::Rect::from_origin_size(
                        origin + offset.to_vec2(),
                        (size.x, size.y),
                    )
                    .inflate(spread, spread);
                    if rect.width() <= 0.0 || rect.height() <= 0.0 {
                        continue;
                    }
                    let radius = (f64::from(radius) + spread).max(0.0) as f32;
                    let command = if sigma == 0.0 {
                        Command::Draw(Box::new(DrawCommand {
                            path: vello_common::kurbo::RoundedRect::from_rect(
                                rect,
                                f64::from(radius),
                            )
                            .to_path(0.1),
                            brush: Brush::Solid(color),
                            stroke: None,
                            fill: Fill::NonZero,
                            transform: self.state.transform,
                            clips: self.state.clips.clone(),
                        }))
                    } else {
                        Command::BoxShadow {
                            rect,
                            radius,
                            sigma,
                            color,
                            transform: self.state.transform,
                            clips: self.state.clips.clone(),
                        }
                    };
                    self.group()?.commands.push(command);
                }
                12 => {
                    if depth >= 8 {
                        return Err(invalid("pictures nest at most 8 levels"));
                    }
                    let resource_base = reader.count(65535)?;
                    let nested_text = reader.count(16_777_216)?;
                    let width = reader.value()?;
                    let height = reader.value()?;
                    if width <= 0.0 || height <= 0.0 {
                        return Err(invalid("nested picture dimensions must be positive"));
                    }
                    let length = reader.count(values.len() - reader.offset)?;
                    let nested = &values[reader.offset..reader.offset + length];
                    reader.offset += length;
                    let nested_assets = assets
                        .get(resource_base..)
                        .ok_or_else(|| invalid("nested picture resources are missing"))?;
                    let saved = self.state.clone();
                    if self.state.clips.len() >= 64 {
                        return Err(invalid("too many nested drawing clips"));
                    }
                    let bounds = vello_common::kurbo::Rect::new(
                        0.0,
                        0.0,
                        f64::from(width),
                        f64::from(height),
                    );
                    self.state
                        .clips
                        .push(self.state.transform * bounds.to_path(0.1));
                    self.run(
                        nested,
                        nested_assets,
                        text_base + nested_text,
                        Some((f64::from(width), f64::from(height))),
                        depth + 1,
                    )?;
                    self.state = saved;
                }
                _ => unreachable!(),
            }
        }
        if self.stack.len() != entry {
            return Err(invalid("unbalanced canvas save/restore"));
        }
        Ok(())
    }
}
