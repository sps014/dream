use super::*;
use crate::tests::fill;

fn pixel(surface: &Surface, x: usize, y: usize, width: usize) -> [u8; 4] {
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    let start = (y * width + x) * 4;
    pixels[start..start + 4].try_into().unwrap()
}

fn render(values: &[f32], assets: &[assets::Asset], text: &str) -> Surface {
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    surface
        .render(&Picture::decode_with(values, assets, text).unwrap())
        .unwrap();
    surface
}

fn nest(values: &mut Vec<f32>, width: f32, height: f32, inner: &[f32]) {
    values.extend([12.0, 0.0, 0.0, width, height, inner.len() as f32]);
    values.extend(inner);
}

#[test]
fn rounded_rect_path_rounds_each_corner_separately() {
    let values = [
        1.0, 1.0, 5.0, 0.0, 0.0, 32.0, 32.0, 16.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0,
    ];
    let surface = render(&values, &[], "");
    assert_eq!(pixel(&surface, 0, 0, 32)[3], 0);
    assert_eq!(pixel(&surface, 31, 0, 32)[3], 255);
    assert_eq!(pixel(&surface, 0, 31, 32)[3], 255);
    assert_eq!(pixel(&surface, 16, 16, 32)[3], 255);

    let mut negative = values;
    negative[7] = -1.0;
    assert!(Picture::decode(&negative).is_err());
    // A rounded rectangle starts its own contour, so it may follow other segments.
    let combined = [
        1.0, 2.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 8.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0,
        1.0, 0.0,
    ];
    assert!(Picture::decode(&combined).is_ok());
}

#[test]
fn text_commands_read_the_text_pool() {
    let values = [
        10.0, 0.0, 5.0, 5.0, 9.0, 20.0, 400.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0,
    ];
    let surface = render(&values, &[], "HelloNoto Sans");
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    assert!(pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 128));

    assert!(Picture::decode_with(&values, &[], "Hello").is_err());
    let mut past_end = values;
    past_end[1] = 10.0;
    assert!(Picture::decode_with(&past_end, &[], "HelloNoto Sans").is_err());
}

#[test]
fn nested_pictures_compose_in_the_current_transform_and_bounds() {
    let mut inner = vec![0.0, 1.0, 0.0, 0.0, 1.0];
    fill(&mut inner, 0.0, 0.0, 32.0, 32.0, [0.0, 0.0, 1.0, 1.0]);
    inner.push(3.0);
    let mut values = vec![5.0, 1.0, 0.0, 0.0, 1.0, 16.0, 0.0];
    nest(&mut values, 8.0, 8.0, &[0.0, 1.0, 0.0, 0.0, 1.0]);
    let surface = render(&values, &[], "");
    assert_eq!(pixel(&surface, 20, 4, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(&surface, 4, 4, 32)[3], 0);
    assert_eq!(pixel(&surface, 20, 12, 32)[3], 0);

    let mut clipped = Vec::new();
    nest(&mut clipped, 8.0, 8.0, &inner[..inner.len() - 1]);
    let surface = render(&clipped, &[], "");
    assert_eq!(pixel(&surface, 4, 4, 32), [0, 0, 255, 255]);
    assert_eq!(pixel(&surface, 12, 12, 32)[3], 0);

    // Clear is only valid first in the nested recording, and save/restore must balance inside it.
    let mut late_clear = Vec::new();
    nest(&mut late_clear, 8.0, 8.0, &inner[5..]);
    late_clear.extend([0.0, 1.0, 0.0, 0.0, 1.0]);
    assert!(Picture::decode(&late_clear).is_err());
    let mut unbalanced = vec![3.0];
    nest(&mut unbalanced, 8.0, 8.0, &[4.0]);
    unbalanced.push(4.0);
    assert!(Picture::decode(&unbalanced).is_err());
    let mut open = Vec::new();
    nest(&mut open, 8.0, 8.0, &inner);
    assert!(Picture::decode(&open).is_err());
}

#[test]
fn nested_pictures_offset_resources_and_text() {
    let image = assets::RasterImage::rgba(1, 1, &[0, 255, 0, 255]).unwrap();
    let inner = [
        9.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 8.0, 8.0, 1.0, 1.0, 10.0, 0.0, 1.0, 1.0, 9.0, 20.0,
        400.0, 0.0, 0.0, 0.0, 10.0, 1.0, 1.0, 1.0, 1.0,
    ];
    let mut values = vec![12.0, 1.0, 3.0, 32.0, 32.0, inner.len() as f32];
    values.extend(inner);
    let assets = [
        assets::Asset::Image(assets::RasterImage::rgba(1, 1, &[255, 0, 0, 255]).unwrap()),
        assets::Asset::Image(image),
    ];
    let surface = render(&values, &assets, "abcxNoto Sans");
    assert_eq!(pixel(&surface, 4, 4, 32), [0, 255, 0, 255]);
    assert!(Picture::decode_with(&values, &assets[..1], "abcxNoto Sans").is_err());
}

#[test]
fn nesting_depth_and_backdrop_budget_cover_nested_pictures() {
    let mut nested = vec![0.0, 0.0, 0.0, 0.0, 1.0];
    for _ in 0..8 {
        let mut outer = Vec::new();
        nest(&mut outer, 4.0, 4.0, &nested);
        nested = outer;
    }
    assert!(Picture::decode(&nested).is_ok());
    let mut deeper = Vec::new();
    nest(&mut deeper, 4.0, 4.0, &nested);
    assert!(Picture::decode(&deeper).is_err());

    let panel = |values: &mut Vec<f32>| {
        values.push(8.0);
        values.extend(crate::tests::rect(0.0, 0.0, 4.0, 4.0));
        values.push(1.0);
        values.extend([0.0; 7]);
    };
    let mut inner = Vec::new();
    for _ in 0..MAX_BACKDROPS {
        panel(&mut inner);
    }
    let mut values = Vec::new();
    nest(&mut values, 32.0, 32.0, &inner);
    assert!(Picture::decode(&values).is_ok());
    panel(&mut values);
    assert!(Picture::decode(&values).is_err());
}

fn blur_panel(values: &mut Vec<f32>, x: f32, y: f32, size: f32, sigma: f32) {
    values.push(8.0);
    values.extend(crate::tests::rect(x, y, size, size));
    values.extend([sigma, 0.0, 0.0, 0.2, 0.3, 0.0, 0.0, 0.0]);
}

#[test]
fn batched_backdrops_match_replaying_every_prefix() {
    let mut full = vec![0.0, 0.0, 0.0, 0.0, 1.0];
    fill(&mut full, 0.0, 0.0, 16.0, 32.0, [1.0, 0.0, 0.0, 1.0]);
    blur_panel(&mut full, 4.0, 4.0, 16.0, 3.0);
    fill(&mut full, 18.0, 10.0, 6.0, 6.0, [0.0, 0.0, 1.0, 1.0]);
    blur_panel(&mut full, 10.0, 8.0, 16.0, 2.0);
    blur_panel(&mut full, 2.0, 14.0, 12.0, 4.0);

    let batched = pollster::block_on(render(&full, &[], "").pixels()).unwrap();
    crate::prepare::UNBATCHED.set(true);
    let unbatched = pollster::block_on(render(&full, &[], "").pixels()).unwrap();
    crate::prepare::UNBATCHED.set(false);
    assert_eq!(batched, unbatched);
}

fn many_panels() -> Picture {
    let mut values = vec![0.0, 0.2, 0.4, 0.6, 1.0];
    for index in 0..MAX_BACKDROPS {
        let offset = (index % 8) as f32 * 12.0;
        fill(&mut values, offset, 0.0, 8.0, 8.0, [1.0, 1.0, 1.0, 1.0]);
        blur_panel(&mut values, offset, (index / 8) as f32 * 6.0, 16.0, 2.0);
    }
    Picture::decode(&values).unwrap()
}

#[test]
#[ignore = "requires a real GPU; absence is a failure"]
fn gpu_many_backdrops_match_cpu_and_reuse_targets() {
    let mut cpu = pollster::block_on(Surface::create(128, 64, 1)).unwrap();
    let mut gpu = pollster::block_on(Surface::create(128, 64, 2)).unwrap();
    let picture = many_panels();
    cpu.render(&picture).unwrap();
    gpu.render(&picture).unwrap();
    let cpu = pollster::block_on(cpu.pixels()).unwrap();
    let pixels = pollster::block_on(gpu.pixels()).unwrap();
    assert!(cpu.iter().zip(&pixels).all(|(a, b)| a.abs_diff(*b) <= 5));
    assert!(gpu.stats().scratch_bytes <= 2 * 128 * 64 * 4);
}

#[test]
fn many_backdrops_reuse_two_snapshot_targets() {
    let mut surface = pollster::block_on(Surface::create(128, 64, 1)).unwrap();
    let started = std::time::Instant::now();
    surface.render(&many_panels()).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    assert!(surface.stats().scratch_bytes <= 2 * 128 * 64 * 4);
}

fn shadow(x: f32, radius: f32, dx: f32, sigma: f32, spread: f32) -> Vec<f32> {
    vec![
        13.0, x, 8.0, 16.0, 16.0, radius, dx, 0.0, sigma, spread, 0.0, 0.0, 0.0, 1.0,
    ]
}

#[test]
fn box_shadow_is_a_soft_offset_rounded_rect() {
    let alpha = |values: &[f32], x: usize, y: usize| pixel(&render(values, &[], ""), x, y, 32)[3];
    let soft = shadow(8.0, 4.0, 0.0, 3.0, 0.0);
    assert!(alpha(&soft, 16, 16) > 240);
    assert!(alpha(&soft, 8, 16) > 64 && alpha(&soft, 8, 16) < 192);
    assert!(alpha(&soft, 5, 16) > 0 && alpha(&soft, 5, 16) < alpha(&soft, 8, 16));
    assert_eq!(alpha(&soft, 0, 16), 0);

    let moved = shadow(8.0, 4.0, 4.0, 3.0, 0.0);
    assert!(alpha(&moved, 26, 16) > alpha(&soft, 26, 16));
    let spread = shadow(8.0, 4.0, 0.0, 3.0, 3.0);
    assert!(alpha(&spread, 6, 16) > alpha(&soft, 6, 16));

    let hard = shadow(8.0, 0.0, 0.0, 0.0, 0.0);
    assert_eq!(alpha(&hard, 8, 8), 255);
    assert_eq!(alpha(&hard, 7, 16), 0);
    assert!(Picture::decode(&shadow(8.0, 0.0, 0.0, 129.0, 0.0)).is_err());
    assert!(Picture::decode(&shadow(8.0, -1.0, 0.0, 2.0, 0.0)).is_err());
    assert!(Picture::decode(&shadow(8.0, 0.0, 0.0, 2.0, -20.0)).is_ok());
}

#[test]
#[ignore = "requires a real GPU; absence is a failure"]
fn gpu_box_shadow_matches_cpu() {
    let mut values = vec![0.0, 1.0, 1.0, 1.0, 1.0];
    values.extend(shadow(8.0, 4.0, 2.0, 3.0, 1.0));
    let picture = Picture::decode(&values).unwrap();
    let mut cpu = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    let mut gpu = pollster::block_on(Surface::create(32, 32, 2)).unwrap();
    cpu.render(&picture).unwrap();
    gpu.render(&picture).unwrap();
    let cpu = pollster::block_on(cpu.pixels()).unwrap();
    let gpu = pollster::block_on(gpu.pixels()).unwrap();
    assert!(cpu.iter().zip(&gpu).all(|(a, b)| a.abs_diff(*b) <= 5));
}
