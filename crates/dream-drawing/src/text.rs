use crate::{DrawingError, Result};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontWeight, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};
use vello_common::peniko::{Blob, FontData};

pub struct GlyphRun {
    pub font: FontData,
    pub size: f32,
    pub coords: Vec<glifo::NormalizedCoord>,
    pub glyphs: Vec<glifo::Glyph>,
}

pub struct ShapedText {
    pub width: f32,
    pub height: f32,
    pub runs: Vec<GlyphRun>,
}

type TextKey = (String, String, u32, u32, u32, u32);

struct TextEngine {
    fonts: FontContext,
    context: LayoutContext<()>,
    cache: BTreeMap<TextKey, Arc<ShapedText>>,
    cache_bytes: usize,
}

impl TextEngine {
    fn new() -> Self {
        let mut fonts = FontContext::new();
        fonts.collection.register_fonts(
            Blob::new(Arc::new(include_bytes!("../fonts/NotoSans.ttf").to_vec())),
            None,
        );
        Self {
            fonts,
            context: LayoutContext::new(),
            cache: BTreeMap::new(),
            cache_bytes: 0,
        }
    }

    fn layout(
        &mut self,
        text: &str,
        family: &str,
        size: f32,
        weight: f32,
        width: f32,
        alignment: u32,
    ) -> Result<Arc<ShapedText>> {
        if text.len() > 262144
            || family.len() > 1024
            || !size.is_finite()
            || !(0.1..=4096.0).contains(&size)
            || !weight.is_finite()
            || !(1.0..=1000.0).contains(&weight)
            || !width.is_finite()
            || !(0.0..=1_000_000.0).contains(&width)
            || alignment > 3
        {
            return Err(DrawingError::Invalid(
                "invalid text size, weight, wrapping width, alignment or text length".into(),
            ));
        }
        let key = (
            text.into(),
            family.into(),
            size.to_bits(),
            weight.to_bits(),
            width.to_bits(),
            alignment,
        );
        if let Some(text) = self.cache.get(&key) {
            return Ok(text.clone());
        }
        let mut builder = self
            .context
            .ranged_builder(&mut self.fonts, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(weight)));
        builder.push_default(StyleProperty::FontFamily(family.into()));
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines((width > 0.0).then_some(width));
        layout.align(
            match alignment {
                1 => Alignment::Center,
                2 => Alignment::End,
                3 => Alignment::Justify,
                _ => Alignment::Start,
            },
            AlignmentOptions::default(),
        );
        let mut runs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(glyphs) = item {
                    let run = glyphs.run();
                    runs.push(GlyphRun {
                        font: run.font().clone(),
                        size: run.font_size(),
                        coords: run
                            .normalized_coords()
                            .iter()
                            .map(|coord| coord.to_bits())
                            .collect(),
                        glyphs: glyphs
                            .positioned_glyphs()
                            .map(|glyph| glifo::Glyph {
                                id: glyph.id,
                                x: glyph.x,
                                y: glyph.y,
                            })
                            .collect(),
                    });
                }
            }
        }
        let shaped = Arc::new(ShapedText {
            width: layout.width(),
            height: layout.height(),
            runs,
        });
        let bytes = text.len()
            + family.len()
            + shaped
                .runs
                .iter()
                .map(|run| run.glyphs.len() * size_of::<glifo::Glyph>())
                .sum::<usize>();
        if self.cache.len() >= 256 || self.cache_bytes + bytes > 16 * 1024 * 1024 {
            self.cache.clear();
            self.cache_bytes = 0;
        }
        if bytes <= 16 * 1024 * 1024 {
            self.cache_bytes += bytes;
            self.cache.insert(key, shaped.clone());
        }
        Ok(shaped)
    }
}

static ENGINE: OnceLock<Mutex<TextEngine>> = OnceLock::new();

fn engine() -> std::sync::MutexGuard<'static, TextEngine> {
    ENGINE
        .get_or_init(|| Mutex::new(TextEngine::new()))
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

pub fn layout(
    text: &str,
    family: &str,
    size: f32,
    weight: f32,
    width: f32,
    alignment: u32,
) -> Result<Arc<ShapedText>> {
    engine().layout(text, family, size, weight, width, alignment)
}

pub fn register(bytes: &[u8]) -> Result<String> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(DrawingError::Invalid("font exceeds 32 MiB".into()));
    }
    {
        let mut engine = engine();
        let fonts = engine
            .fonts
            .collection
            .register_fonts(Blob::new(Arc::new(bytes.to_vec())), None);
        let family = fonts
            .first()
            .and_then(|(id, _)| engine.fonts.collection.family_name(*id).map(str::to_string))
            .ok_or_else(|| {
                DrawingError::Invalid("no supported font faces in the supplied bytes".into())
            })?;
        engine.cache.clear();
        engine.cache_bytes = 0;
        Ok(family)
    }
}
