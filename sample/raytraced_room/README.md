# ION — neon district

An interactive cyberpunk street rendered by Dream raytracing shaders, for native
desktop and browser WASM. Staggered towers, illuminated windows, layered neon
signs, an overhead span and a low vehicle frame a street-level view. Wet asphalt
breaks reflected signage into fine ripples. Distance haze, colored light spill,
subtle light animation and filmic tone mapping establish a late-night palette.

The scene is a compact, stylized study inspired by neon city imagery. Facade
panels, windows and sign lettering are shaded procedurally; there are no imported
city meshes, network assets or JavaScript rendering dependencies.

## Build and run

Use the compiler and host libraries from this checkout. From the repository root,
prepare them once:

```sh
cargo build --workspace
./target/debug/dreamer toolchain install slang
```

Windows PowerShell uses `./target/debug/dreamer.exe`. Native Direct3D 12 shader
compilation also needs the Windows SDK's `dxc` on PATH. Metal compilation needs
macOS. Each build keeps shader targets appropriate to its platform.

Run the native sample from the repository root:

```sh
./target/debug/dream run sample/raytraced_room/main.dream
```

Build and serve the browser version:

```sh
./target/debug/dream build --runtime --web sample/raytraced_room/main.dream
python -m http.server 8787 --bind 127.0.0.1 --directory sample/raytraced_room
```

Open <http://localhost:8787>. WebGPU needs an available GPU adapter and localhost
or HTTPS. Reload after rebuilding. Outputs live in the sample's ignored `target/`
directory; the page reports missing builds and startup failures.

Add `--release` to optimize the application. From this sample directory,
`dreamer run --target native` and `dreamer run --target web` also build and launch
the respective application.

## Controls

Click the browser scene to focus its keyboard input.

| Input | Action |
| --- | --- |
| Left mouse drag / arrow keys | Look around |
| W / S / A / D | Move forward / backward / left / right |
| Q / E | Move down / up |
| Shift | Move faster |
| Space | Pause or resume lighting animation |
| R | Restore the initial camera and restart animation |
| H | Switch hardware/portable tracing on a ray-query device |
| Escape | Close the scene; reload the browser page to reopen |

This is a free camera: it can pass through buildings and the vehicle. Movement
stays within the scene bounds. Resizing updates the aspect ratio and surface;
minimized windows yield instead of rendering a zero-sized frame. Frame-time
clamping prevents large camera jumps after a stall.

## Rendering and troubleshooting

Native devices use hardware ray queries when supported. The application tries
the compiled graphics backends and prefers one with `GpuFeature.RayQuery`;
otherwise it uses the first available portable backend. Browser builds use
WebGPU and software triangle intersections running on the GPU.

Both modes trace the same static mesh and use the same materials, lighting and
camera. Hardware mode builds acceleration structures once. H changes pipelines
without rebuilding geometry. Both renderers support shadow rays and two
reflection bounces. Animated sign intensity and lighting do not move geometry.

A moving point light casts hard shadows; additional colored fill and glow
approximate scattered neon light. Wet-road ripples perturb reflection directions.
This is direct lighting with artistic fill, not a progressive path tracer or a
physically complete global-illumination renderer.

Shared GPU records use `GpuVec4` fields to keep CPU, WGSL and native layouts
consistent. The sign font is a small embedded bitmap buffer, rasterized from
Pillow's bundled font. To regenerate it after changing the words, install Pillow,
run `python sample/raytraced_room/make_sign_font.py`, then format the generated
Dream file with `dream fmt sample/raytraced_room/room_signs.dream`.

Hardware-only shader entry points are deliberately excluded from WASM. The
compiler warning about `room_hardware` being unavailable on WASM is expected;
do not add WebGPU to that shader's targets.

Backend creation failures are logged while other candidates are tried. Errors
after selection, such as acceleration-structure or pipeline failures, are
reported instead of silently changing renderers. Native errors appear in the
terminal; browser errors appear in the page and developer console. Check that
the compiler and hosts are from the same checkout, Slang is installed, and the
GPU driver supports the selected API.

## Source and performance

`room_scene.dream` builds geometry. `room_shading.dream` contains materials,
procedural detail, sign sampling, lighting and display encoding.
`room_shaders.dream` contains portable intersections, hardware queries and shader
entries. `room_acceleration.dream` builds the portable spatial hierarchy. `main.dream` owns resources, input and presentation. JavaScript launches
the WASM application and displays its status.

Portable mode builds a bounding-volume hierarchy once and skips subtrees whose
bounds a ray cannot hit. Leaves contain up to four triangle indices; geometry and
triangle ordering are unchanged. Native hardware shadow queries stop after their
first opaque hit. Resolution and visible reflections still affect performance. Start with the default 960 × 600 window
and reduce its size if necessary. The terminal and browser overlay report FPS
and drawable resolution about every three seconds. These are delivered frames,
including presentation and vsync, rather than a GPU-only benchmark.

## Acceleration checks

Run the sample's focused tests from the repository root:

```sh
./target/debug/dream test sample/raytraced_room/acceleration_tests.dream --native
```

The tests check tree bounds, escape links, and complete triangle coverage. They
also compare 6,804 GPU rays against an independent exhaustive triangle search,
checking exact nearest-hit indices and distances plus shadow visibility. Cases
include seeded random rays, axis-parallel and near-parallel directions, zero
directions, short ranges, triangle vertices and rays starting on surfaces.
Unavailable backends are reported; at least one must run for the test to pass.
The exhaustive oracle is confined to the test file.

## Validation on 2026-10-25

The optimized scene keeps all 220 triangles, four antialiasing samples per pixel,
two reflection bounces, shading, textures and full drawable resolution. The
portable renderer uses a deterministic median-split hierarchy, with conservative
bounds and the original triangle-index tie break. Its construction uses the
standard library's sorting implementation.

A paired headless Microsoft Edge run on Windows with an NVIDIA GeForce RTX 4080
(NVIDIA Lovelace WebGPU adapter) measured these delivered frame rates at
1280 × 800, using debug WASM builds:

| Camera | Exhaustive search | Spatial hierarchy | Speedup |
| --- | ---: | ---: | ---: |
| Initial street view | 57.7 FPS | 110.9 FPS | 1.92× |
| Side view | 63.7 FPS | 120.8 FPS | 1.90× |
| Upward view | 78.9 FPS | 157.6 FPS | 2.00× |
| Downward view | 50.3 FPS | 89.1 FPS | 1.77× |

The benchmark counted GPU queue submissions over three seconds after warming each
view. Animation time was fixed and camera input synchronized between runs. All
four 1280 × 800 captures were pixel-identical: zero changed pixels and zero channel
difference. Headless presentation differs from a visible vsynced window; these
results are specific to the tested hardware and views.

Both focused tests passed. Each of Direct3D 12, Vulkan and WebGPU matched the
exhaustive oracle for all 6,804 rays, including exact hit distances. The optimized
browser also passed startup, stable paused images, movement, mouse look, reset,
continuous rotation, unavailable hardware-query messaging, resizing and shutdown
checks, with no browser errors. Separate checks verified messages for unavailable
WebGPU and a missing WASM build.

Native and browser builds passed. Linux/Vulkan and macOS/Metal application
execution were not available for this validation; Vulkan GPU query tests ran on
Windows. The Dream formatter check, documentation unit tests, API inventory,
generated API-page synchronization check and strict MkDocs build passed. No
compiler, stdlib or runtime sources were changed.
