use crate::assets::RasterImage;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};
use vello_common::pixmap::Pixmap;

#[derive(Clone)]
pub struct Noise {
    pub image: Arc<RasterImage>,
    pub opacity: f32,
}

static TILES: OnceLock<Mutex<BTreeMap<u32, Arc<RasterImage>>>> = OnceLock::new();

pub fn tile(seed: u32) -> Arc<RasterImage> {
    let mut tiles = TILES
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(tile) = tiles.get(&seed) {
        return tile.clone();
    }
    let mut rng = fastrand::Rng::with_seed(u64::from(seed));
    let mut pixels = Pixmap::new(64, 64);
    for pixel in pixels.data_mut() {
        let gray = rng.u32(0..=255) as u8;
        *pixel = vello_common::peniko::color::PremulRgba8 {
            r: gray,
            g: gray,
            b: gray,
            a: 255,
        };
    }
    pixels.recompute_may_have_transparency();
    let image = Arc::new(RasterImage {
        pixels: Arc::new(pixels),
    });
    if tiles.len() == 32 {
        tiles.pop_first();
    }
    tiles.insert(seed, image.clone());
    image
}
