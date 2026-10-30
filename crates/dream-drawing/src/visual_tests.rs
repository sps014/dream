use crate::{
    Picture, Surface,
    tests::{fill, rect},
};

fn styled_picture() -> Picture {
    let mut values = vec![2., 2., 0., 8., 16., 1., 24., 16.];
    values.extend([1., 8., 16., 24., 16., 4.]);
    for (offset, color) in [
        (0., [1., 0., 0., 1.]),
        (0.5, [1., 0., 0., 1.]),
        (0.5, [0., 0., 1., 1.]),
        (1., [0., 0., 1., 1.]),
    ] {
        values.push(offset);
        values.extend(color);
    }
    values.extend([8., 1., 1., 4., 0., 0.]);
    Picture::decode(&values).unwrap()
}

#[test]
fn gradient_stroke_has_hard_stops_and_round_caps() {
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    surface.render(&styled_picture()).unwrap();
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    let at = |x: usize, y: usize| &pixels[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
    assert_eq!(at(10, 16), [255, 0, 0, 255]);
    assert_eq!(at(22, 16), [0, 0, 255, 255]);
    assert!(at(5, 16)[3] > 200);
    assert_eq!(at(3, 16)[3], 0);
}

#[test]
fn blend_modes_are_isolated_and_match_channel_equations() {
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    let d = 0.25_f32;
    let s = 0.75_f32;
    let expected = [
        s,
        s * d,
        1. - (1. - s) * (1. - d),
        2. * s * d,
        d,
        s,
        s - d,
        s + d - 2. * s * d,
    ];
    for (mode, value) in expected.into_iter().enumerate() {
        let mut values = Vec::new();
        fill(&mut values, 0., 0., 32., 32., [d, d, d, 1.]);
        values.extend([7., 0.5, mode as f32, 0.]);
        fill(&mut values, 0., 0., 24., 32., [s, s, s, 1.]);
        fill(&mut values, 8., 0., 24., 32., [s, s, s, 1.]);
        values.push(4.);
        surface.render(&Picture::decode(&values).unwrap()).unwrap();
        let pixels = pollster::block_on(surface.pixels()).unwrap();
        let expected = ((value * 0.5 + d * 0.5) * 255.).round() as u8;
        assert!(
            pixels[(16 * 32 + 16) * 4].abs_diff(expected) <= 2,
            "mode {mode}"
        );
    }
}

#[test]
fn malformed_styles_and_gradient_stops_return_errors() {
    for values in [
        vec![7., 1., 8., 4.],
        vec![1., 0., 3., 0., 0., 1., 0., 2.],
        vec![1., 0., 1., 0., 0., 32., 0., 1.],
    ] {
        assert!(Picture::decode(&values).is_err());
    }
    let mut values = vec![1.];
    values.extend(rect(0., 0., 32., 32.));
    values.extend([
        1., 0., 0., 32., 0., 2., 0.7, 1., 0., 0., 1., 0.2, 0., 0., 1., 1., 0.,
    ]);
    assert!(Picture::decode(&values).is_err());
}

#[test]
#[ignore = "requires a real GPU; absence is a failure"]
fn styled_cpu_gpu_output_matches() {
    let mut cpu = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    let mut gpu = pollster::block_on(Surface::create(32, 32, 2)).unwrap();
    let mut pictures = vec![styled_picture()];
    for mode in 0..8 {
        let mut values = Vec::new();
        fill(&mut values, 0., 0., 32., 32., [0.25, 0.5, 0.75, 1.]);
        values.extend([7., 0.6, mode as f32, 0.]);
        fill(&mut values, 4., 4., 24., 24., [0.7, 0.3, 0.8, 0.8]);
        values.push(4.);
        pictures.push(Picture::decode(&values).unwrap());
    }
    for picture in pictures {
        cpu.render(&picture).unwrap();
        gpu.render(&picture).unwrap();
        let cpu = pollster::block_on(cpu.pixels()).unwrap();
        let gpu = pollster::block_on(gpu.pixels()).unwrap();
        assert!(cpu.iter().zip(gpu).all(|(a, b)| a.abs_diff(b) <= 5));
    }
}
