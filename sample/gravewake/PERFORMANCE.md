# Measurements

Measured on 2026-10-20 (local time), Windows 11, Intel Core i9-13900K and
NVIDIA GeForce RTX 4080 16 GB, using the release build. These are whole frame-loop
intervals after 30 warmup frames, with 180 measured intervals and vsync requested
off. They include CPU work and presentation; they are not GPU timestamps.

| Mode | Drawable | Internal resolution | Mean ms | p95 ms | p99 ms |
| --- | --- | --- | ---: | ---: | ---: |
| Raster High | 1920x1080 | 1920x1080 | 4.17 | 4.27 | 12.88 |
| Raster Ultra | 1920x1080 | 3840x2160 | 4.17 | 4.31 | 4.37 |

These short measurements include winter terrain, displaced snow, living humanoids
and hounds, sun shadows, planar reflections, refraction and scene anti-aliasing.
Presentation pacing dominates these results; do not infer isolated GPU cost or
RT overhead. High had a 12.88 ms p99 interval in this run.

A 60-second native combat stress run cycled all four presets, switched all three
weapons, fired, reloaded, threw grenades and triggered shockwaves. It completed
14,621 measured frames with no GPU error or descriptor assertion: mean 4.104 ms,
p95 4.480 ms, p99 5.786 ms. The workload replenishes health and ammunition.

The world has 26,848 static triangles and 151 authored collision boxes. Near
humanoids have 1,248 triangles and three appearances; hounds have 1,140 triangles.
Living animation uses 64 baked poses per appearance. The combined three-weapon
viewmodel has 3,000 triangles. Deaths use 22 humanoid or 18 hound joints, two
physics substeps and six constraint solves, with a twelve-actor cap.

Winter adds a persistent 1024-square RGBA16Float compression field, a
320x352-cell snow surface (225,280 triangles), a 512-square CPU field, a bounded
384-event stamp queue and 320 falling flakes. The weather layer is at scene
resolution. Raster uses static chunk and actor frustum culling, bounded enemy
and particle pools, a 2048-square sun shadow map and a 960x540 planar reflection.
Refraction adds one scene color/depth copy at internal resolution. Optional native
RT retains its depth prepass.

Native and WebGPU release builds compile. Browser checks exercised pointer
capture, three weapon selections, magnified scope, firing, reloading, grenades,
walking trails, mute, Escape pause, audio suspension and resizing. The Web Audio
probe observed 30 buffer-source starts and a running context after interaction,
then a suspended context after Escape. This confirms event delivery and audio
scheduling; it is not a listening-quality assessment. No browser or GPU errors
were reported. Native audio is not wired. Headless checks do not substitute for
a long manual survival session or native minimize/focus testing. Linux/macOS
execution remains unverified.

No new game test cases or compiler/runtime corpus runs were added for this
sample-only iteration. Documentation checks run independently.

Logs, captures and raw measurements stay in the ignored
`target/gravewake-assets/` directory. Reproduce short native measurements with:

```sh
python -B sample/gravewake/tools/benchmark_native.py --case high --case ultra
```

Measured binary SHA-256: `b4dc1cf7d55289e73bd199b73caf2a05f78d1c420c5d9852f04ea98a97cfc9d2`.

## Browser audio and scoped-fire follow-up

The mouse-button fix was checked against the real Chromium event stream: while
RMB remained held, three LMB presses reduced the sniper magazine from five to two.
Releasing aim before fire also worked, and reloading restored five rounds. Pause
suspended the AudioContext; clicking resumed it. The music/effects probe observed
104 buffer-source starts during the interaction check and no browser/GPU errors.
These are interaction and signal checks, not a subjective listening assessment.

Offline renders of early and capped late-wave arrangements contained finite,
unclipped samples. The later arrangement increases rhythmic density while music
RMS remains close to the early mix. All ten effect categories also rendered
without invalid or clipped samples. Audio synthesis caps active voices at 96.
Prior native frame measurements above precede this browser audio follow-up;
they do not measure audio-thread cost.
