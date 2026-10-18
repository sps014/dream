# Pelagic — Dream Seascape

A GPU ocean that runs from the same Dream source in a native window and a browser.
It adapts [Seascape by Alexander Alekseev (TDM), 2014](https://www.shadertoy.com/view/Ms2SD1),
using the [author's source](https://github.com/tdmaav/shadertoy/blob/master/Seascape.shader).
The shader traces a procedural height field, with five octaves for surface normals,
sky reflections, light scattering and specular highlights. This is a height-field ocean,
rather than a Gerstner mesh.

## Build and run

Use the compiler and GPU/window hosts built from this checkout: older installed Dream
versions may lack the GPU API. From the repository root, build the workspace once:

```sh
cargo build --workspace
```

Install the pinned shader compiler once with the freshly built toolchain manager
(`dreamer.exe` on Windows):

```sh
./target/debug/dreamer toolchain install slang
```

From the repository root, run the native version:

```sh
./target/debug/dream run sample/ocean/main.dream
```

Build the browser version and serve the sample:

```sh
./target/debug/dream build --runtime --web sample/ocean/main.dream
python -m http.server 8787 --bind 127.0.0.1 --directory sample/ocean
```

Open <http://localhost:8787>. Use a browser with WebGPU enabled and an available GPU
adapter. WebGPU requires localhost or HTTPS. The page displays startup errors, and
the native app prints window/GPU failures to the terminal. Generated artifacts live
under this sample's ignored `target/` directory; build them before opening the page.
No external textures, JavaScript packages or network assets are needed at run time.

Alternatively, from this directory, `dreamer run --target native` and
`dreamer run --target web` build and launch the selected version.

On Windows PowerShell, from `sample/ocean`, the equivalent explicit commands are:

```powershell
..\..\target\debug\dream.exe run main.dream
..\..\target\debug\dream.exe build --runtime --web main.dream
python -m http.server 8787 --bind 127.0.0.1
```

Rebuild the compiler and forward its arguments with `cargo run --bin dream -- ...`.
For example, from this directory:

```sh
cargo run --bin dream -- build --runtime --web main.dream
```

If VS Code Live Server shows only the title, try the Python server above. The page
shows whether it is loading the runtime, downloading WASM, compiling, or opening
the GPU, and reports failed or timed-out downloads. A browser page must be reloaded
after rebuilding; an already running ocean keeps its previously loaded shaders.

## Optimized download and size

For release distribution, build from the repository root:

```sh
./target/debug/dream build --release --runtime --web -Oz sample/ocean/main.dream
```

`-Oz` prioritizes a smaller binary. Omit `--release -Oz` while iterating to avoid
the extra optimization time. The Rust compiler executable can remain a debug build;
the flags above optimize the Dream program it produces.

Measured for this sample with `--release --runtime --web -Oz`:

| Artifact | Uncompressed | Gzip | Brotli |
| --- | ---: | ---: | ---: |
| `main.wasm` | 350.7 KiB | 65.9 KiB | 50.2 KiB |
| `main.web.runtime.js` | 157.8 KiB | 33.9 KiB | 29.5 KiB |
| Combined | 508.5 KiB | 99.8 KiB | 79.7 KiB |

Sizes depend on the compiler and toolchain revision. The uncompressed WASM contains:

| Part | Size | Contents |
| --- | ---: | --- |
| CPU code | 311.6 KiB | Application control flow and reachable runtime/stdlib code: window/input tracking, GPU command recording, async scheduling, memory management, strings and errors |
| Data | 33.4 KiB | Shader bundle, strings, constants and runtime tables |
| Other sections and metadata | 5.7 KiB | Imports, exports, types, function tables and embedded host ABI |

The shader bundle is **12.1 KiB**, included in the data row. Wave evaluation runs on
the GPU, not in the WASM CPU code. There are no ocean meshes or texture assets to
download. The current CPU-side runtime and stdlib code dominate the binary size;
`-Oz` reduces that code but does not remove the services this program reaches.

For comparison, building the music-player sample with the same flags produces
55.0 KiB of WASM, including 39.2 KiB of CPU code. It delegates the DOM, audio playback
and media decoding to browser APIs through `js`. This sample compiles a shared
native/browser window, input and GPU control path instead. A separate window/input
probe without ocean rendering measures 116.2 KiB. These are different sets of
reachable services, rather than a fixed runtime cost paid by every Dream program.

The compiler also writes `.wasm.gz`, `.wasm.br` and compressed JavaScript files.
Production hosting must select these files through content negotiation and send the
matching `Content-Encoding: gzip` or `Content-Encoding: br` header to obtain those
transfer sizes. Python's basic HTTP server serves the uncompressed files. The large
`.wat` file is a text inspection artifact; this page does not download it, the Slang
source or the CMake/native build cache.

## Controls

Click the ocean in the browser to give it keyboard focus.

| Key | Action |
| --- | --- |
| Space | Pause/resume waves and camera drift |
| Up / Down | Increase/decrease wave height |
| Left / Right | Warmer/cooler daylight |
| Q | Toggle nine-sample antialiasing (enabled initially) |
| R | Reset time, wave height and daylight |
| Escape | Close the ocean; reload the browser page to restart |

The render loop follows the display refresh rate. Nine subpixel samples smooth the
fine crests and highlights. Press Q for a faster single-sample mode on slower GPUs.
Rendering cost scales with the drawable pixel count: reduce the window size on
high-DPI screens if needed.
The program handles resize and skips rendering while minimized.

## Attribution and license

The shader adaptation in `main.dream` retains the original's
[Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported license](https://creativecommons.org/licenses/by-nc-sa/3.0/).
Credit Alexander Alekseev (TDM), link the original and license, identify changes,
and distribute adaptations under that license for noncommercial use.

This port expresses the original noise, choppy octaves, height tracing, normals and
lighting in Dream. It uses explicit shader parameters and one shared height sampler,
retains the original camera rotation and forward drift, skips tracing above the
horizon, enables the original's optional nine-sample antialiasing, and adds keyboard
controls, resizing and a browser host page.

The noise lattice uses the public-domain
[lowbias32 integer hash](https://github.com/skeeto/hash-prospector) instead of the
original sine hash. This avoids native GPU floating-point differences causing visible
breaks between noise cells; individual wave shapes differ from the original.
The output also accounts for sRGB window surfaces so native presentation does not
apply a second display conversion and wash out the water.
