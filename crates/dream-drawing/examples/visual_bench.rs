#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use dream_drawing::{Picture, Surface};
    use std::time::Instant;
    fn fill(v: &mut Vec<f32>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        v.extend([
            1.,
            5.,
            0.,
            x,
            y,
            1.,
            x + w,
            y,
            1.,
            x + w,
            y + h,
            1.,
            x,
            y + h,
            4.,
            0.,
        ]);
        v.extend(color);
        v.push(0.);
    }
    let mut reports = Vec::new();
    for (backend, code) in [("cpu", 1), ("gpu", 2)] {
        for kind in ["unfiltered", "filtered", "acrylic"] {
            let mut values = vec![0., 0.1, 0.2, 0.3, 1.];
            if kind == "filtered" {
                values.extend([
                    7., 1., 0., 3., 1., 0., 8., 12., 0., 0., 0., 0.4, 2., 1.1, 4., 0.8,
                ]);
            }
            for i in 0..8 {
                fill(
                    &mut values,
                    20. + i as f32 * 28.,
                    24.,
                    24.,
                    160.,
                    [0.2, 0.4, 0.8, 0.8],
                );
            }
            if kind == "filtered" {
                values.push(4.);
            }
            if kind == "acrylic" {
                for x in [12., 112., 212.] {
                    // Reuse the rectangle encoding as a panel clip without recording a fill.
                    let mut path = Vec::new();
                    fill(&mut path, x, 32., 88., 160., [0.; 4]);
                    values.push(8.);
                    values.extend_from_slice(&path[1..15]);
                    values.extend([8., 1., 1., 1., 0.1]);
                }
            }
            let picture = Picture::decode(&values)?;
            let mut surface = pollster::block_on(Surface::create(320, 240, code))?;
            let start = Instant::now();
            surface.render(&picture)?;
            let cold_us = start.elapsed().as_secs_f64() * 1e6;
            let before = surface.stats();
            let start = Instant::now();
            for _ in 0..30 {
                surface.render(&picture)?;
            }
            let warm_us = start.elapsed().as_secs_f64() * 1e6 / 30.;
            let after = surface.stats();
            reports.push(serde_json::json!({"backend":backend,"scene":kind,"width":320,"height":240,"frames":30,"cold_cpu_submission_us":cold_us,"warm_cpu_submission_us":warm_us,"cold_scratch_allocations":before.scratch_allocations,"warm_scratch_allocations":after.scratch_allocations-before.scratch_allocations,"resident_scratch_bytes":after.scratch_bytes,"gpu_pixel_reads_during_render":after.pixel_reads-before.pixel_reads}));
            // Completion is deliberately outside CPU submission timing.
            pollster::block_on(surface.pixels())?;
        }
    }
    println!("{}", serde_json::to_string_pretty(&reports)?);
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
