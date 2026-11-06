use crate::{DrawingError, Result};
use lru::LruCache;
use parley::{
    Affinity, Alignment, AlignmentOptions, BoundingBox, Cursor, FontContext, FontWeight, Layout,
    LayoutContext, PositionedLayoutItem, Selection, StyleProperty,
};
use std::sync::{Arc, Mutex, OnceLock};
use vello_common::peniko::{Blob, FontData};

const CACHE_ENTRIES: usize = 256;
const CACHE_BYTES: usize = 16 * 1024 * 1024;

pub struct GlyphRun {
    pub font: FontData,
    pub size: f32,
    pub coords: Vec<glifo::NormalizedCoord>,
    pub glyphs: Vec<glifo::Glyph>,
}

/// Shaped paragraph. Guest text positions are UTF-16 code unit offsets, like Dream strings;
/// Parley works in UTF-8 byte offsets, so every query converts at this boundary.
pub struct ShapedText {
    pub width: f32,
    pub height: f32,
    pub runs: Vec<GlyphRun>,
    text: String,
    layout: Layout<()>,
}

impl ShapedText {
    pub fn line_count(&self) -> usize {
        self.layout.len()
    }

    /// Distance from the top to the first line's baseline, or 0 for empty text.
    pub fn baseline(&self) -> f32 {
        self.layout
            .get(0)
            .map_or(0.0, |line| line.metrics().baseline)
    }

    /// `[width, height, line count, first baseline]`.
    pub fn metrics(&self) -> [f32; 4] {
        [
            self.width,
            self.height,
            self.line_count() as f32,
            self.baseline(),
        ]
    }

    /// The text position nearest to a point in layout coordinates.
    pub fn index_at(&self, x: f32, y: f32) -> usize {
        let cursor = Cursor::from_point(&self.layout, x, y);
        self.utf16_index(cursor.index())
    }

    /// `[x, y, width, height]` of a caret one unit wide before the UTF-16 offset `index`.
    pub fn caret(&self, index: usize) -> [f32; 4] {
        let cursor =
            Cursor::from_byte_index(&self.layout, self.byte_index(index), Affinity::Downstream);
        rect(cursor.geometry(&self.layout, 1.0))
    }

    /// `[x, y, width, height]` for each line fragment covering UTF-16 offsets `start..end`.
    pub fn selection(&self, start: usize, end: usize) -> Vec<f32> {
        let anchor =
            Cursor::from_byte_index(&self.layout, self.byte_index(start), Affinity::Downstream);
        let focus = Cursor::from_byte_index(&self.layout, self.byte_index(end), Affinity::Upstream);
        Selection::new(anchor, focus)
            .geometry(&self.layout)
            .into_iter()
            .flat_map(|(bounds, _)| rect(bounds))
            .collect()
    }

    /// Clamps to the text and moves an offset inside a surrogate pair to the pair's start.
    fn byte_index(&self, utf16: usize) -> usize {
        let mut units = 0;
        for (byte, character) in self.text.char_indices() {
            let next = units + character.len_utf16();
            if next > utf16 {
                return byte;
            }
            units = next;
        }
        self.text.len()
    }

    fn utf16_index(&self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        self.text
            .char_indices()
            .take_while(|(start, _)| *start < byte)
            .map(|(_, character)| character.len_utf16())
            .sum()
    }

    fn cache_bytes(&self, family: &str) -> usize {
        // The layout keeps clusters and glyphs alongside the runs; count both copies.
        2 * self.text.len()
            + family.len()
            + 2 * self
                .runs
                .iter()
                .map(|run| run.glyphs.len() * size_of::<glifo::Glyph>())
                .sum::<usize>()
    }
}

fn rect(bounds: BoundingBox) -> [f32; 4] {
    [
        bounds.x0 as f32,
        bounds.y0 as f32,
        (bounds.x1 - bounds.x0) as f32,
        (bounds.y1 - bounds.y0) as f32,
    ]
}

type TextKey = (String, String, u32, u32, u32, u32);

struct TextEngine {
    fonts: FontContext,
    context: LayoutContext<()>,
    cache: LruCache<TextKey, (Arc<ShapedText>, usize)>,
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
            cache: LruCache::unbounded(),
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
        if let Some((text, _)) = self.cache.get(&key) {
            return Ok(text.clone());
        }
        let shaped = Arc::new(self.shape(text, family, size, weight, width, alignment));
        let bytes = shaped.cache_bytes(family);
        if bytes <= CACHE_BYTES {
            self.cache_bytes += bytes;
            self.cache.put(key, (shaped.clone(), bytes));
            while self.cache.len() > CACHE_ENTRIES || self.cache_bytes > CACHE_BYTES {
                let Some((_, (_, evicted))) = self.cache.pop_lru() else {
                    break;
                };
                self.cache_bytes -= evicted;
            }
        }
        Ok(shaped)
    }

    fn shape(
        &mut self,
        text: &str,
        family: &str,
        size: f32,
        weight: f32,
        width: f32,
        alignment: u32,
    ) -> ShapedText {
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
        ShapedText {
            width: layout.width(),
            height: layout.height(),
            runs,
            text: text.into(),
            layout,
        }
    }

    fn clear_cache(&mut self) {
        self.cache.clear();
        self.cache_bytes = 0;
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
    engine.clear_cache();
    Ok(family)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "text_tests.rs"]
mod tests;
