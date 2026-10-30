use crate::{Picture, Surface, tests::fill};

fn filtered(effects: &[f32], opacity: f32, color: [f32; 4]) -> Picture {
    let mut values = vec![7., opacity, 0.];
    values.extend(effects);
    fill(&mut values, 8., 8., 12., 12., color);
    values.push(4.);
    Picture::decode(&values).unwrap()
}

#[test]
fn ordered_color_effects_preserve_alpha_and_snapshot_pixels() {
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    for (effects, expected) in [
        (vec![2., 2., 2., 3., 0.], 64_u8),
        (vec![2., 3., 0., 2., 2.], 128),
    ] {
        surface
            .render(&filtered(&effects, 1., [1., 0., 0., 0.5]))
            .unwrap();
        let pixels = pollster::block_on(surface.pixels()).unwrap();
        let at = &pixels[(12 * 32 + 12) * 4..(12 * 32 + 12) * 4 + 4];
        assert!(at[..3].iter().all(|value| value.abs_diff(expected) <= 1));
        assert_eq!(at[3], 128);
        assert_eq!(&pixels[..4], &[0; 4]);
    }
    surface
        .render(&filtered(&[1., 4., 0.], 0.5, [1., 0., 0., 1.]))
        .unwrap();
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    let at = &pixels[(12 * 32 + 12) * 4..(12 * 32 + 12) * 4 + 4];
    assert!(at[..3].iter().all(|value| value.abs_diff(27) <= 1));
    assert_eq!(at[3], 128);
}

#[test]
fn spatial_filters_work_and_cpu_targets_are_reused() {
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    let picture = filtered(
        &[2., 1., 8., 0., 2., 0., 0., 1., 0.5, 0., 1.],
        1.,
        [1., 0., 0., 1.],
    );
    surface.render(&picture).unwrap();
    let first = pollster::block_on(surface.pixels()).unwrap();
    assert!(first[(14 * 32 + 25) * 4 + 2] > 10);
    let allocations = surface.stats().scratch_allocations;
    for _ in 0..3 {
        surface.render(&picture).unwrap();
    }
    assert_eq!(surface.stats().scratch_allocations, allocations);
    assert_eq!(first, pollster::block_on(surface.pixels()).unwrap());
}

#[test]
fn invalid_effect_chains_are_errors() {
    for values in [
        vec![7., 1., 0., 17.],
        vec![7., 1., 0., 1., 0., 129., 4.],
        vec![7., 1., 0., 1., 2., -1., 4.],
        vec![7., 1., 0., 1., 4., 9., 4.],
        vec![7., 1., 0., 1., 1., 9000., 0., 1., 0., 0., 0., 1., 4.],
    ] {
        assert!(Picture::decode(&values).is_err());
    }
}

#[test]
#[ignore = "requires a real GPU; absence is a failure"]
fn gpu_filters_match_cpu_reuse_targets_and_never_read_pixels_during_render() {
    let mut cpu = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    let mut gpu = pollster::block_on(Surface::create(32, 32, 2)).unwrap();
    for effects in [
        vec![3., 2., 0.7, 3., 1.2, 4., 0.3],
        vec![2., 1., 5., 3., 2., 0., 0., 1., 0.4, 0., 2.],
    ] {
        let picture = filtered(&effects, 0.7, [0.8, 0.2, 0.1, 0.6]);
        cpu.render(&picture).unwrap();
        let reads = gpu.stats().pixel_reads;
        gpu.render(&picture).unwrap();
        assert_eq!(gpu.stats().pixel_reads, reads);
        let allocations = gpu.stats().scratch_allocations;
        for _ in 0..3 {
            gpu.render(&picture).unwrap();
        }
        assert_eq!(gpu.stats().scratch_allocations, allocations);
        let a = pollster::block_on(cpu.pixels()).unwrap();
        let b = pollster::block_on(gpu.pixels()).unwrap();
        let max = a.iter().zip(b).map(|(a, b)| a.abs_diff(b)).max().unwrap();
        assert!(max <= 6, "CPU/GPU difference {max}");
    }
}

#[test]
fn filtered_output_respects_the_saved_clip() {
    let mut values = vec![6.];
    values.extend(crate::tests::rect(8., 8., 16., 16.));
    values.extend([7., 0.5, 0., 2., 0., 3., 2., 0.5]);
    fill(&mut values, 0., 0., 32., 32., [1., 1., 1., 1.]);
    values.push(4.);
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    surface.render(&Picture::decode(&values).unwrap()).unwrap();
    let pixels = pollster::block_on(surface.pixels()).unwrap();
    assert_eq!(pixels[(16 * 32 + 4) * 4 + 3], 0);
    assert!(pixels[(16 * 32 + 16) * 4 + 3] > 100);
    assert_eq!(pixels[(16 * 32 + 28) * 4 + 3], 0);
}

#[test]
fn group_clips_apply_antialiasing_only_once() {
    let mut surface = pollster::block_on(Surface::create(32, 32, 1)).unwrap();
    let mut expected = Vec::new();
    for grouped in [false, true] {
        let mut values = vec![6., 4., 0., 0., 0., 1., 32., 0., 1., 0., 32., 4.];
        if grouped {
            values.extend([7., 1., 0., 0.]);
        }
        fill(&mut values, 0., 0., 32., 32., [1.; 4]);
        if grouped {
            values.push(4.);
        }
        surface.render(&Picture::decode(&values).unwrap()).unwrap();
        let pixels = pollster::block_on(surface.pixels()).unwrap();
        if grouped {
            assert_eq!(pixels, expected);
        } else {
            expected = pixels;
        }
    }
}
