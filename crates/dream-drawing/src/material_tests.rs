use crate::{
    Picture, Surface,
    tests::{fill, rect},
};

fn panel(values: &mut Vec<f32>, sigma: f32, noise: f32, seed: u32) {
    values.push(8.);
    values.extend(rect(0., 0., 128., 32.));
    values.extend([
        sigma,
        0.,
        0.,
        0.,
        0.,
        noise,
        (seed & 65535) as f32,
        (seed >> 16) as f32,
    ]);
}
fn nested(with_panel: bool) -> Picture {
    let mut values = Vec::new();
    fill(&mut values, 0., 0., 128., 32., [1., 0., 0., 1.]);
    values.extend([7., 0.5, 0., 0.]);
    fill(&mut values, 0., 0., 128., 32., [0., 1., 0., 0.4]);
    values.extend([7., 0.6, 0., 0.]);
    fill(&mut values, 0., 0., 128., 32., [0., 0., 1., 0.5]);
    if with_panel {
        panel(&mut values, 0., 0., 0);
    }
    values.extend([4., 4.]);
    Picture::decode(&values).unwrap()
}
fn grain(seed: u32, translate: bool) -> Picture {
    let mut values = Vec::new();
    fill(&mut values, 0., 0., 128., 32., [0.4, 0.4, 0.4, 0.5]);
    if translate {
        values.extend([5., 1., 0., 0., 1., 16., 0.]);
        values.push(8.);
        values.extend(rect(-16., 0., 128., 32.));
        values.extend([
            0.,
            0.,
            0.,
            0.,
            0.,
            0.3,
            (seed & 65535) as f32,
            (seed >> 16) as f32,
        ]);
    } else {
        panel(&mut values, 0., 0.3, seed);
    }
    Picture::decode(&values).unwrap()
}
fn blurred_nested() -> Picture {
    let mut values = Vec::new();
    fill(&mut values, 0., 0., 128., 32., [1., 0., 0., 1.]);
    values.extend([7., 0.5, 0., 0.]);
    fill(&mut values, 0., 0., 128., 32., [0., 0., 0., 1.]);
    fill(&mut values, 64., 0., 64., 32., [1.; 4]);
    panel(&mut values, 3., 0.04, 42);
    fill(&mut values, 60., 12., 8., 8., [1., 0., 0., 1.]);
    values.push(4.);
    Picture::decode(&values).unwrap()
}
#[test]
fn nested_zero_blur_does_not_resample_parent_or_double_alpha() {
    let mut surface = pollster::block_on(Surface::create(128, 32, 1)).unwrap();
    surface.render(&nested(false)).unwrap();
    let expected = pollster::block_on(surface.pixels()).unwrap();
    surface.render(&nested(true)).unwrap();
    assert_eq!(expected, pollster::block_on(surface.pixels()).unwrap());
}
#[test]
fn noise_is_deterministic_device_anchored_and_preserves_alpha() {
    let mut surface = pollster::block_on(Surface::create(128, 32, 1)).unwrap();
    surface.render(&grain(0xfedc1234, false)).unwrap();
    let first = pollster::block_on(surface.pixels()).unwrap();
    assert!(
        first
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 128 && p[0] == p[1] && p[1] == p[2])
    );
    assert_eq!(
        &first[(8 * 128 + 8) * 4..(8 * 128 + 8) * 4 + 4],
        &first[(8 * 128 + 72) * 4..(8 * 128 + 72) * 4 + 4]
    );
    assert!(
        first
            .as_chunks::<4>()
            .0
            .windows(2)
            .any(|p| p[0][0] != p[1][0])
    );
    surface.render(&grain(0xfedc1234, true)).unwrap();
    assert_eq!(first, pollster::block_on(surface.pixels()).unwrap());
    surface.render(&grain(9, false)).unwrap();
    assert_ne!(first, pollster::block_on(surface.pixels()).unwrap());
    surface.render(&grain(0xfedc1234, false)).unwrap();
    assert_eq!(first, pollster::block_on(surface.pixels()).unwrap());
}
#[test]
fn nested_blur_excludes_later_drawing_and_global_panel_budget_is_enforced() {
    let mut surface = pollster::block_on(Surface::create(128, 32, 1)).unwrap();
    surface.render(&blurred_nested()).unwrap();
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    let at = (16 * 128 + 64) * 4;
    assert_eq!(&pixels[at..at + 4], &[255, 0, 0, 255]);
    assert!(pixels[(8 * 128 + 62) * 4 + 1] > 5);
    let mut values = Vec::new();
    for _ in 0..9 {
        values.extend([7., 1., 0., 0.]);
        panel(&mut values, 0., 0., 0);
        values.push(4.);
    }
    assert!(Picture::decode(&values).is_err());
}
#[test]
#[ignore = "requires a real GPU; absence is a failure"]
fn gpu_materials_match_cpu_and_remain_stable() {
    let mut cpu = pollster::block_on(Surface::create(128, 32, 1)).unwrap();
    let mut gpu = pollster::block_on(Surface::create(128, 32, 2)).unwrap();
    for picture in [
        nested(true),
        clipped_panel(),
        grain(0xfedc1234, false),
        grain(0xfedc1234, true),
        blurred_nested(),
    ] {
        cpu.render(&picture).unwrap();
        let reads = gpu.stats().pixel_reads;
        gpu.render(&picture).unwrap();
        assert_eq!(gpu.stats().pixel_reads, reads);
        let a = pollster::block_on(cpu.pixels()).unwrap();
        let b = pollster::block_on(gpu.pixels()).unwrap();
        let max = a.iter().zip(&b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        assert!(max <= 6, "CPU/GPU difference {max}");
        gpu.render(&picture).unwrap();
        assert_eq!(b, pollster::block_on(gpu.pixels()).unwrap());
    }
}

fn clipped_panel() -> Picture {
    let mut values = Vec::new();
    fill(&mut values, 0., 0., 128., 32., [0.2, 0.4, 0.6, 0.5]);
    values.push(6.);
    values.extend(rect(0., 0., 96., 32.));
    values.push(8.);
    values.extend(rect(16., 4., 48., 24.));
    values.extend([0.; 8]);
    Picture::decode(&values).unwrap()
}

#[test]
fn backdrop_intersects_clips_without_erasing_other_content() {
    let mut surface = pollster::block_on(Surface::create(128, 32, 1)).unwrap();
    let mut values = Vec::new();
    fill(&mut values, 0., 0., 128., 32., [0.2, 0.4, 0.6, 0.5]);
    surface.render(&Picture::decode(&values).unwrap()).unwrap();
    let expected = pollster::block_on(surface.pixels()).unwrap();
    surface.render(&clipped_panel()).unwrap();
    let actual = pollster::block_on(surface.pixels()).unwrap();
    assert!(
        actual == expected,
        "a clipped zero-blur panel changed pixels outside its intersection"
    );
}
