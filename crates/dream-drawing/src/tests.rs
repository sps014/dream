use super::*;
use crate::assets::{Asset, RasterImage};

pub(super) fn rect(x: f32, y: f32, width: f32, height: f32) -> Vec<f32> {
    vec![
        5.0,
        0.0,
        x,
        y,
        1.0,
        x + width,
        y,
        1.0,
        x + width,
        y + height,
        1.0,
        x,
        y + height,
        4.0,
    ]
}

pub(super) fn fill(
    values: &mut Vec<f32>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    values.push(1.0);
    values.extend(rect(x, y, width, height));
    values.push(0.0);
    values.extend(color);
    values.push(0.0);
}

fn backdrop(values: &mut Vec<f32>, sigma: f32) {
    values.push(8.0);
    values.extend(rect(8.0, 4.0, 16.0, 24.0));
    values.push(sigma);
    values.extend([0.0; 4]);
}

#[test]
fn malformed_commands_are_rejected() {
    for values in [
        vec![1.0],
        vec![4.0],
        vec![3.0],
        vec![9.0],
        vec![f32::NAN],
        vec![1.5],
        vec![7.0, 2.0],
    ] {
        assert!(Picture::decode(&values).is_err());
    }
    assert!(dimensions(0, 1).is_err());
    assert!(dimensions(8192, 8192).is_err());
}

#[test]
fn cpu_backdrop_is_clipped_and_excludes_later_content() {
    let mut values = vec![0.0, 0.0, 0.0, 0.0, 1.0];
    fill(&mut values, 16.0, 0.0, 16.0, 32.0, [1.0; 4]);
    backdrop(&mut values, 3.0);
    fill(&mut values, 14.0, 12.0, 4.0, 8.0, [1.0, 0.0, 0.0, 1.0]);
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    surface.render(&Picture::decode(&values).unwrap()).unwrap();
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    let at = |x: usize, y: usize| &pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
    assert_eq!(at(4, 16), [0, 0, 0, 255]);
    assert_eq!(at(28, 16), [255; 4]);
    assert!(at(14, 8)[0] > 0 && at(14, 8)[0] < 255);
    assert_eq!(at(16, 16), [255, 0, 0, 255]);
}

#[test]
fn zero_blur_does_not_double_translucent_alpha() {
    let mut values = Vec::new();
    fill(&mut values, 0.0, 0.0, 32.0, 32.0, [1.0, 0.0, 0.0, 0.5]);
    backdrop(&mut values, 0.0);
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    surface.render(&Picture::decode(&values).unwrap()).unwrap();
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    assert_eq!(
        &pixels[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4],
        &[128, 0, 0, 128]
    );
}

#[test]
fn opacity_group_applies_once_and_render_replaces_pixels() {
    let mut values = vec![7.0, 0.5, 0.0, 0.0];
    fill(&mut values, 0.0, 0.0, 24.0, 32.0, [1.0, 0.0, 0.0, 1.0]);
    fill(&mut values, 8.0, 0.0, 24.0, 32.0, [1.0, 0.0, 0.0, 1.0]);
    values.push(4.0);
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    surface.render(&Picture::decode(&values).unwrap()).unwrap();
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    assert_eq!(
        &pixels[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4],
        &[128, 0, 0, 128]
    );
    surface.render(&Picture::decode(&[]).unwrap()).unwrap();
    assert!(
        pollster::block_on(surface.pixels())
            .unwrap()
            .iter()
            .all(|byte| *byte == 0)
    );
}

#[test]
fn backdrop_rejects_unsupported_nesting_and_excessive_blur() {
    let mut nested = vec![7.0, 1.0, 0.0, 0.0];
    backdrop(&mut nested, 3.0);
    nested.push(4.0);
    assert!(Picture::decode(&nested).is_err());
    let mut excessive = Vec::new();
    backdrop(&mut excessive, 129.0);
    assert!(Picture::decode(&excessive).is_err());
}

fn image_picture() -> Picture {
    let image = RasterImage::rgba(2, 1, &[255, 0, 0, 255, 0, 255, 0, 128]).unwrap();
    let values = [9.0, 0.0, 0.0, 0.0, 2.0, 1.0, 0.0, 0.0, 32.0, 32.0, 1.0, 1.0];
    Picture::decode_with(&values, &[Asset::Image(image)]).unwrap()
}

#[test]
fn image_scaling_and_straight_alpha_conversion() {
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    surface.render(&image_picture()).unwrap();
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    assert_eq!(
        &pixels[(16 * 32 + 4) * 4..(16 * 32 + 4) * 4 + 4],
        &[255, 0, 0, 255]
    );
    assert_eq!(
        &pixels[(16 * 32 + 28) * 4..(16 * 32 + 28) * 4 + 4],
        &[0, 128, 0, 128]
    );
    assert!(RasterImage::rgba(2, 2, &[0; 3]).is_err());
    assert!(RasterImage::decode(b"not an image").is_err());
}

#[test]
fn png_decoding_uses_dimensions_and_alpha() {
    let source = image::RgbaImage::from_raw(1, 1, vec![255, 0, 0, 128]).unwrap();
    let mut encoded = std::io::Cursor::new(Vec::new());
    source
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let image = RasterImage::decode(encoded.get_ref()).unwrap();
    assert_eq!(image.pixels.width(), 1);
    assert_eq!(image.pixels.height(), 1);
    assert_eq!(image.pixels.data()[0].r, 128);
}

#[test]
fn shaped_text_wraps_and_renders_with_the_bundled_font() {
    let line = text::layout("office नमस्ते", "Noto Sans", 20.0, 400.0, 0.0, 0).unwrap();
    assert!(line.width > 20.0 && line.height >= 20.0);
    assert!(
        line.runs
            .iter()
            .flat_map(|run| &run.glyphs)
            .all(|glyph| glyph.id != 0)
    );
    let wrapped = text::layout("office office office", "Noto Sans", 20.0, 400.0, 70.0, 0).unwrap();
    assert!(wrapped.height > line.height);
    assert!(text::layout("hello", "Noto Sans", f32::NAN, 400.0, 0.0, 0).is_err());
    assert!(text::register(b"not a font").is_err());
    assert_eq!(
        text::register(include_bytes!("../fonts/NotoSans.ttf")).unwrap(),
        "Noto Sans"
    );
    let picture = Picture::decode_with(
        &[11.0, 0.0, 4.0, 2.0, 1.0, 1.0, 1.0, 1.0],
        &[Asset::Text(line)],
    )
    .unwrap();
    let mut surface = pollster::block_on(Surface::create(200, 50, 1)).unwrap();
    surface.render(&picture).unwrap();
    assert!(
        pollster::block_on(surface.pixels())
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 128)
    );
}

#[test]
#[ignore = "requires a real GPU; absence is a failure"]
fn gpu_images_and_text_match_cpu() {
    let line = text::layout("office", "Noto Sans", 20.0, 400.0, 0.0, 0).unwrap();
    let mut picture = image_picture();
    picture.commands.extend(
        Picture::decode_with(
            &[11.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0],
            &[Asset::Text(line)],
        )
        .unwrap()
        .commands,
    );
    let mut cpu = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    let mut gpu = pollster::block_on(Surface::create(32, 32, 2)).unwrap();
    cpu.render(&picture).unwrap();
    gpu.render(&picture).unwrap();
    let cpu = pollster::block_on(cpu.pixels()).unwrap();
    let first = pollster::block_on(gpu.pixels()).unwrap();
    assert!(cpu.iter().zip(&first).all(|(a, b)| a.abs_diff(*b) <= 5));
    gpu.render(&picture).unwrap();
    assert_eq!(first, pollster::block_on(gpu.pixels()).unwrap());
}

#[test]
#[ignore = "requires a real GPU; absence is a failure"]
fn gpu_backdrop_matches_cpu() {
    let mut values = vec![0.0, 0.0, 0.0, 0.0, 1.0];
    fill(&mut values, 16.0, 0.0, 16.0, 32.0, [1.0; 4]);
    backdrop(&mut values, 3.0);
    let picture = Picture::decode(&values).unwrap();
    let mut cpu = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    let mut gpu = pollster::block_on(Surface::create(32, 32, 2)).unwrap();
    cpu.render(&picture).unwrap();
    gpu.render(&picture).unwrap();
    let cpu = pollster::block_on(cpu.pixels()).unwrap();
    let gpu = pollster::block_on(gpu.pixels()).unwrap();
    assert!(cpu.iter().zip(gpu).all(|(a, b)| a.abs_diff(b) <= 5));
}
