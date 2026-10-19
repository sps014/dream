//! Native backend matrix for dream-host-gpu, driven through the host API without a guest.
//!
//! The parent runs device-free checks, then re-runs this binary once per backend so each
//! backend gets its own process: its own loader state for the strict-AOT check, its own Vulkan
//! environment, and a main thread for window surfaces. Backends this machine or build cannot
//! run are reported as skipped. `DREAM_GPU_SURFACE_TEST=1` adds a real window surface check
//! (needs a desktop session), like the window host's `DREAM_WINDOW_DISPLAY_TEST`.

use dream_host_gpu::backend::Backend;
use dream_host_gpu::error::{self, Code, HostResult};
use dream_host_gpu::{bundle, devices, guest, pipelines, resources, submit, surfaces, testing};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

static SAXPY: &[u8] = include_bytes!("fixtures/saxpy.dgpb");
static TRIANGLE: &[u8] = include_bytes!("fixtures/triangle.dgpb");
static GENERATED: &[u8] = include_bytes!("fixtures/generated.dgpb");
static QUAD: &[u8] = include_bytes!("fixtures/quad.dgpb");

static RESULTS: Mutex<Option<HashMap<usize, u64>>> = Mutex::new(None);
static ARRIVED: Condvar = Condvar::new();
static NEXT_FUTURE: AtomicUsize = AtomicUsize::new(1);

fn on_complete(future: usize, value: u64) {
    let mut results = RESULTS.lock().unwrap();
    results
        .get_or_insert_with(HashMap::new)
        .insert(future, value);
    ARRIVED.notify_all();
}

fn future() -> usize {
    NEXT_FUTURE.fetch_add(1, Ordering::Relaxed)
}

fn wait(future: usize) -> u64 {
    let mut results = RESULTS.lock().unwrap();
    loop {
        if let Some(value) = results.as_mut().and_then(|r| r.remove(&future)) {
            return value;
        }
        let (next, timeout) = ARRIVED
            .wait_timeout(results, Duration::from_secs(60))
            .unwrap();
        assert!(!timeout.timed_out(), "future {future} never completed");
        results = next;
    }
}

fn last_code() -> String {
    error::last_text().lines().next().unwrap_or("").to_string()
}

fn expect_err<T>(result: HostResult<T>, code: Code, what: &str) {
    match result {
        Ok(_) => panic!("{what}: expected {}", code.as_str()),
        Err(e) => assert_eq!(e.code, code, "{what}: {}", e.detail),
    }
}

/// Mirrors the stdlib's GpuCommandEncoder word stream.
#[derive(Default)]
struct Commands {
    words: Vec<i32>,
    data: Vec<u8>,
}

impl Commands {
    fn op(&mut self, code: i32, operands: &[i32]) -> &mut Self {
        self.words.push(code);
        self.words.extend_from_slice(operands);
        self
    }

    fn value(&mut self, stage: i32, index: i32, bytes: &[u8]) -> &mut Self {
        let offset = self.data.len() as i32;
        self.data.extend_from_slice(bytes);
        self.op(5, &[stage, index, 6, offset, bytes.len() as i32])
    }

    fn buffer(&mut self, stage: i32, index: i32, handle: i32) -> &mut Self {
        self.op(5, &[stage, index, 1, handle, 0])
    }

    fn submit(&self, device: i32) -> Result<(), String> {
        let id = future();
        submit::submit(id, device, &self.words, &self.data);
        match wait(id) {
            0 => Ok(()),
            _ => Err(error::last_text()),
        }
    }
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn read_buffer(handle: i32, offset: i32, size: i32) -> Vec<u8> {
    let id = future();
    resources::buffer_read(id, handle, offset, size);
    unsafe { guest::test_bytes(wait(id)) }
}

fn saxpy(device: i32) -> Result<(), String> {
    let library = pipelines::library_load(device, SAXPY).map_err(|e| e.text())?;
    let pipeline = pipelines::compute_pipeline_create(library, "saxpy").map_err(|e| e.text())?;
    let n: i32 = 1000;
    let x: Vec<f32> = (0..n).map(|i| i as f32 * 0.5).collect();
    let y: Vec<f32> = (0..n).map(|i| 100.0 - i as f32).collect();
    let bytes = 4 * n;
    let xb = resources::buffer_create(device, bytes, 1).map_err(|e| e.text())?;
    let yb = resources::buffer_create(device, bytes, 1).map_err(|e| e.text())?;
    let out = resources::buffer_create(device, bytes, 1).map_err(|e| e.text())?;
    resources::buffer_write(xb, 0, &floats(&x)).map_err(|e| e.text())?;
    resources::buffer_write(yb, 0, &floats(&y)).map_err(|e| e.text())?;
    let a = 2.5f32;
    let mut commands = Commands::default();
    commands
        .op(2, &[])
        .op(4, &[pipeline])
        .buffer(0, 0, xb)
        .buffer(0, 1, yb)
        .buffer(0, 2, out)
        .value(0, 3, &a.to_le_bytes())
        .value(0, 4, &n.to_le_bytes())
        .op(6, &[(n + 63) / 64, 1, 1])
        .op(3, &[]);
    // Released while the GPU may still run the dispatch: the host keeps them alive until the
    // submission's fence completes.
    let id = future();
    submit::submit(id, device, &commands.words, &commands.data);
    testing::gpuRelease(xb);
    testing::gpuRelease(yb);
    if wait(id) != 0 {
        return Err(format!("saxpy submit: {}", error::last_text()));
    }
    check_saxpy(out, &x, &y, a)?;
    for handle in [out, pipeline, library] {
        testing::gpuRelease(handle);
    }
    Ok(())
}

fn check_saxpy(out: i32, x: &[f32], y: &[f32], a: f32) -> Result<(), String> {
    let result = read_buffer(out, 0, 4 * x.len() as i32);
    if result.len() != 4 * x.len() {
        return Err(format!(
            "readback returned {} bytes: {}",
            result.len(),
            error::last_text()
        ));
    }
    for (i, chunk) in result.as_chunks::<4>().0.iter().enumerate() {
        let got = f32::from_le_bytes(*chunk);
        let expected = a * x[i] + y[i];
        if (got - expected).abs() > 1e-3 {
            return Err(format!("result[{i}] = {got}, expected {expected}"));
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    testing::set_completion_hook(on_complete);
    if let Some(at) = args.iter().position(|a| a == "--loader-self-test") {
        std::process::exit(loader_self_test(&args[at + 1]));
    }
    if let Some(at) = args.iter().position(|a| a == "--strict-aot-self-test") {
        std::process::exit(strict_aot_self_test(&args[at + 1]));
    }
    if let Some(at) = args.iter().position(|a| a == "--backend") {
        let backend = args[at + 1].parse::<i32>().unwrap();
        std::process::exit(child(Backend::from_i32(backend).unwrap()));
    }
    parent(&args[0]);
}

fn parent(exe: &str) {
    device_free_checks();
    match slang_compiler_library() {
        Some(library) => {
            let status = Command::new(exe)
                .args(["--loader-self-test", &library.to_string_lossy()])
                .status()
                .expect("running the loader self-test");
            assert!(
                status.success(),
                "the loader check missed a mapped Slang compiler"
            );
            println!(
                "loader self-test: a deliberately mapped {} is detected",
                library.display()
            );
            let output = Command::new(exe)
                .args(["--strict-aot-self-test", &library.to_string_lossy()])
                .env("DREAM_GPU_STRICT_AOT", "1")
                .output()
                .expect("running the strict-AOT self-test");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                !output.status.success() && stderr.contains("DREAM_GPU_STRICT_AOT"),
                "DREAM_GPU_STRICT_AOT did not stop a process with a mapped Slang compiler: {stderr}"
            );
            println!("strict-AOT self-test: device creation aborts once Slang is mapped");
        }
        None => println!("loader self-test skipped: no Slang toolchain (set DREAM_SLANG)"),
    }
    run_backends(exe);
}

fn run_backends(exe: &str) {
    let mut failed = Vec::new();
    println!("dream-host-gpu backend matrix:");
    for backend in [
        Backend::Metal,
        Backend::Vulkan,
        Backend::WebGpu,
        Backend::Dx12,
        Backend::Dx11,
        Backend::Cuda,
    ] {
        let mut command = Command::new(exe);
        command.args(["--backend", &(backend as i32).to_string()]);
        if backend == Backend::Vulkan {
            vulkan_environment(&mut command);
        }
        let output = command.output().expect("running a backend child");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let verdict = stdout
            .lines()
            .rev()
            .find(|l| l.starts_with("RESULT "))
            .unwrap_or("RESULT fail: the child printed no result")
            .trim_start_matches("RESULT ")
            .to_string();
        println!("  {:<7} {verdict}", backend.name());
        if !output.status.success() || verdict.starts_with("fail") {
            failed.push(backend.name());
            println!("{stdout}{}", String::from_utf8_lossy(&output.stderr));
        }
    }
    assert!(failed.is_empty(), "backends failed: {failed:?}");
    println!("dream-host-gpu backend checks passed");
}

/// The Slang compiler library of the pinned toolchain, used only to prove the loader check.
fn slang_compiler_library() -> Option<PathBuf> {
    let root = std::env::var_os("DREAM_SLANG")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|h| PathBuf::from(h).join(".dream/toolchains/slang-2026.20"))
        })?;
    let name = if cfg!(target_os = "macos") {
        "lib/libslang-compiler.dylib"
    } else if cfg!(windows) {
        "bin/slang-compiler.dll"
    } else {
        "lib/libslang-compiler.so"
    };
    Some(root.join(name)).filter(|p| p.is_file())
}

#[cfg(unix)]
fn loader_self_test(library: &str) -> i32 {
    if testing::loaded_slang_compilers().0 != 0 {
        return 1;
    }
    let path = std::ffi::CString::new(library).unwrap();
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        return 1;
    }
    if testing::loaded_slang_compilers().0 > 0 {
        0
    } else {
        1
    }
}

#[cfg(windows)]
fn loader_self_test(library: &str) -> i32 {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{
        LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LoadLibraryExW,
    };

    if testing::loaded_slang_compilers().0 != 0 {
        return 1;
    }
    let path: Vec<u16> = std::ffi::OsStr::new(library)
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Slang's companion DLLs live next to the compiler, outside the test executable's PATH.
    let handle = unsafe {
        LoadLibraryExW(
            path.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        )
    };
    if handle.is_null() {
        eprintln!("loading {library}: {}", std::io::Error::last_os_error());
        return 1;
    }
    if testing::loaded_slang_compilers().0 > 0 {
        0
    } else {
        1
    }
}

/// Maps the Slang compiler, then opens a device: with `DREAM_GPU_STRICT_AOT` set the host must
/// end the process before this returns.
fn strict_aot_self_test(library: &str) -> i32 {
    if loader_self_test(library) != 0 {
        return 1;
    }
    let _ = devices::create_now(0, 0);
    0
}

/// Selection and validation paths that need no GPU.
fn device_free_checks() {
    expect_err(
        devices::create_now(4, 1 << 3),
        Code::NotCompiled,
        "Metal not compiled",
    );
    expect_err(devices::create_now(42, 0), Code::Invalid, "unknown backend");
    let foreign = if cfg!(windows) { 4 } else { 1 };
    match devices::create_now(foreign, 0) {
        Err(e) => {
            assert_eq!(e.code, Code::Unavailable, "{}", e.detail);
            assert!(e.detail.contains("built without"), "{}", e.detail);
        }
        Ok(_) => panic!("a backend for another platform opened"),
    }
    expect_err(devices::backend(123_456), Code::Invalid, "unknown device");
    expect_err(
        resources::buffer_create(123_456, 4, 1),
        Code::Invalid,
        "unknown device",
    );
    let id = future();
    resources::buffer_read(id, 123_456, 0, 4);
    assert!(unsafe { guest::test_bytes(wait(id)) }.is_empty());
    assert_eq!(last_code(), "EINVALID");
    let id = future();
    submit::submit(id, 123_456, &[2, 3], &[]);
    assert_eq!(wait(id), 1);
}

/// On macOS, Vulkan needs the Khronos loader in front of MoltenVK; use the probe's pinned
/// copies from the GPU cache unless the environment already names a loader.
fn vulkan_environment(command: &mut Command) {
    if !cfg!(target_os = "macos") || std::env::var_os("VK_DRIVER_FILES").is_some() {
        return;
    }
    let cache = PathBuf::from(env!("DREAM_GPU_CACHE_DIR"));
    let loader = find(&cache, "libvulkan.1.dylib", "vulkan-loader");
    let icd = find(&cache, "MoltenVK_icd.json", "moltenvk");
    if let (Some(loader), Some(icd)) = (loader, icd) {
        command
            .env("DYLD_LIBRARY_PATH", loader.parent().unwrap())
            .env("VK_DRIVER_FILES", icd);
    }
}

fn find(root: &Path, name: &str, under: &str) -> Option<PathBuf> {
    let mut stack: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(under))
        })
        .collect();
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            stack.extend(std::fs::read_dir(&path).ok()?.flatten().map(|e| e.path()));
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name)
            && (name != "MoltenVK_icd.json" || path.to_string_lossy().contains("dynamic"))
        {
            return Some(path);
        }
    }
    None
}

fn child(backend: Backend) -> i32 {
    if !testing::backend_compiled(backend as i32) {
        println!(
            "RESULT skip: this host was built without {}",
            backend.name()
        );
        return 0;
    }
    // The committed bundles come from a machine without dxc, fxc or NVRTC.
    let required: &[&[u8]] = if backend == Backend::Cuda {
        &[SAXPY, GENERATED]
    } else {
        &[SAXPY, TRIANGLE, GENERATED, QUAD]
    };
    let compiled = required
        .iter()
        .all(|b| bundle::parse(b).is_ok_and(|b| b.target_for(backend).is_some()));
    if !compiled {
        println!(
            "RESULT skip: the test bundles have no {} code",
            backend.name()
        );
        return 0;
    }
    let mask = 1 << (backend as i32);
    let device = match devices::create_now(backend as i32, mask) {
        Ok(device) => device,
        Err(e) => {
            println!(
                "RESULT skip: no {} device here ({})",
                backend.name(),
                e.detail
            );
            return 0;
        }
    };
    let outcome = std::panic::catch_unwind(|| run_suite(backend, device));
    let (compilers, image) = testing::loaded_slang_compilers();
    match outcome {
        Ok(Ok(summary)) if compilers == 0 => {
            println!("RESULT pass: {summary}; no Slang compiler image was mapped");
            0
        }
        Ok(Ok(_)) => {
            println!("RESULT fail: the Slang compiler was loaded ({image})");
            1
        }
        Ok(Err(e)) => {
            println!("RESULT fail: {e}");
            1
        }
        Err(_) => {
            println!("RESULT fail: the suite panicked");
            1
        }
    }
}

fn run_suite(backend: Backend, device: i32) -> Result<String, String> {
    assert_eq!(devices::backend(device).unwrap(), backend as i32);
    let features = devices::features(device).unwrap();
    devices::check(device).map_err(|e| e.text())?;
    println!("CHECK saxpy");
    saxpy(device)?;
    println!("CHECK entry parameters");
    scale(device)?;
    println!("CHECK generated compute");
    generated(device)?;
    if backend != Backend::Cuda {
        triangle(device)?;
        quad(device)?;
    }
    errors(device)?;
    let deferred = testing::deferred_releases();
    let surface_check = if backend == Backend::Cuda {
        "CUDA is compute only"
    } else if std::env::var_os("DREAM_GPU_SURFACE_TEST").is_some() {
        surface(device)?;
        "3 frames presented to a real window, resized"
    } else {
        "window surface skipped (set DREAM_GPU_SURFACE_TEST=1)"
    };
    device_loss(device)?;
    let checks = if backend == Backend::Cuda {
        "saxpy, entry-parameter and generator-style compute and error mapping verified"
    } else {
        "saxpy, entry-parameter and generator-style compute, offscreen triangle, textured indexed quad with depth and error mapping verified"
    };
    Ok(format!(
        "{checks}; {surface_check} (features {features:#010b}, {deferred} deferred releases left)"
    ))
}

fn scale(device: i32) -> Result<(), String> {
    let library = pipelines::library_load(device, SAXPY).map_err(|e| e.text())?;
    let pipeline = pipelines::compute_pipeline_create(library, "scale").map_err(|e| e.text())?;
    let values: Vec<f32> = (0..100).map(|i| i as f32).collect();
    let data = resources::buffer_create(device, 400, 1).map_err(|e| e.text())?;
    resources::buffer_write(data, 0, &floats(&values)).map_err(|e| e.text())?;
    let mut commands = Commands::default();
    commands
        .op(2, &[])
        .op(4, &[pipeline])
        .buffer(0, 0, data)
        .value(0, 1, &3.0f32.to_le_bytes())
        .value(0, 2, &90i32.to_le_bytes())
        .op(6, &[2, 1, 1])
        .op(3, &[]);
    commands.submit(device)?;
    let result = read_buffer(data, 0, 400);
    for (i, chunk) in result.as_chunks::<4>().0.iter().enumerate() {
        let got = f32::from_le_bytes(*chunk);
        let expected = if i < 90 { i as f32 * 3.0 } else { i as f32 };
        if got != expected {
            return Err(format!("scale[{i}] = {got}, expected {expected}"));
        }
    }
    if result.len() != 400 {
        return Err(format!("scale readback returned {} bytes", result.len()));
    }
    for handle in [data, pipeline, library] {
        testing::gpuRelease(handle);
    }
    Ok(())
}

/// The generator's shape: Dream parameter names in the bundle, `<entry>_<param>` globals in
/// Slang, so only binding numbers connect them; both entries run in one submission.
fn generated(device: i32) -> Result<(), String> {
    let library = pipelines::library_load(device, GENERATED).map_err(|e| e.text())?;
    let fill = pipelines::compute_pipeline_create(library, "fill").map_err(|e| e.text())?;
    let offset = pipelines::compute_pipeline_create(library, "offset").map_err(|e| e.text())?;
    let data = resources::buffer_create(device, 64 * 4, 1).map_err(|e| e.text())?;
    let mut commands = Commands::default();
    commands
        .op(2, &[])
        .op(4, &[fill])
        .buffer(0, 0, data)
        .value(0, 1, &1000u32.to_le_bytes())
        .value(0, 2, &48i32.to_le_bytes())
        .op(6, &[2, 1, 1])
        .op(3, &[])
        .op(2, &[])
        .op(4, &[offset])
        .buffer(0, 0, data)
        .value(0, 1, &7u32.to_le_bytes())
        .op(6, &[2, 1, 1])
        .op(3, &[]);
    commands.submit(device)?;
    let result = read_buffer(data, 0, 64 * 4);
    if result.len() != 64 * 4 {
        return Err(format!(
            "generated readback returned {} bytes",
            result.len()
        ));
    }
    for (i, chunk) in result.as_chunks::<4>().0.iter().enumerate() {
        let got = u32::from_le_bytes(*chunk);
        let expected = if i < 48 { 1000 + i as u32 + 7 } else { 7 };
        if got != expected {
            return Err(format!("generated[{i}] = {got}, expected {expected}"));
        }
    }
    for handle in [data, fill, offset, library] {
        testing::gpuRelease(handle);
    }
    Ok(())
}

fn triangle(device: i32) -> Result<(), String> {
    let library = pipelines::library_load(device, TRIANGLE).map_err(|e| e.text())?;
    // Rgba8Unorm, triangle list, no culling, replace, no depth, 1 sample, no vertex buffers.
    let desc = [0, 0, 0, 0, 0, 0, 0, 1, 0];
    let pipeline = pipelines::render_pipeline_create(
        library,
        "triangle_vertex",
        library,
        "triangle_fragment",
        &desc,
    )
    .map_err(|e| e.text())?;
    let (width, height) = (64, 32);
    let target =
        resources::texture_create(device, &[width, height, 1, 1, 0, 1, 1]).map_err(|e| e.text())?;
    let bits = |v: f32| v.to_bits() as i32;
    let mut commands = Commands::default();
    commands
        .op(
            1,
            &[
                target,
                1,
                bits(0.0),
                bits(0.0),
                bits(1.0),
                bits(1.0),
                0,
                0,
                0,
            ],
        )
        .op(4, &[pipeline])
        .value(2, 0, &floats(&[0.5, 1.0, 1.0, 1.0]))
        .op(11, &[3, 1, 0, 0])
        .op(3, &[]);
    commands.submit(device)?;
    let id = future();
    resources::texture_read(id, target, 0, 0);
    let pixels = unsafe { guest::test_bytes(wait(id)) };
    if pixels.len() != (width * height * 4) as usize {
        return Err(format!(
            "texture readback returned {} bytes: {}",
            pixels.len(),
            error::last_text()
        ));
    }
    for (i, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
        let red_ok = (126..=129).contains(&pixel[0]);
        if !red_ok || pixel[1] != 0 || pixel[2] != 0 || pixel[3] != 255 {
            return Err(format!(
                "pixel {i} is {pixel:?}, expected [127/128, 0, 0, 255]"
            ));
        }
    }
    for handle in [target, pipeline, library] {
        testing::gpuRelease(handle);
    }
    Ok(())
}

/// Vertex and index buffers, a depth attachment, a texture and a sampler: the explicit encoder
/// path `GpuFrame` and `GpuRenderPass` record.
fn quad(device: i32) -> Result<(), String> {
    let library = pipelines::library_load(device, QUAD).map_err(|e| e.text())?;
    // Rgba8Unorm, triangle list, no culling, replace, Depth32Float (10 + 1), Less, depth write,
    // 1 sample, one vertex buffer: stride 16, per vertex, position (Float32x2 at 0) and uv
    // (Float32x2 at 8).
    let desc = [0, 0, 0, 0, 11, 1, 1, 1, 1, 16, 0, 2, 0, 1, 0, 1, 1, 8];
    let pipeline =
        pipelines::render_pipeline_create(library, "quad_vertex", library, "quad_fragment", &desc)
            .map_err(|e| e.text())?;
    let vertices = floats(&[
        -1.0, -1.0, 0.0, 1.0, //
        1.0, -1.0, 1.0, 1.0, //
        1.0, 1.0, 1.0, 0.0, //
        -1.0, 1.0, 0.0, 0.0,
    ]);
    let vertex_buffer =
        resources::buffer_create(device, vertices.len() as i32, 2).map_err(|e| e.text())?;
    resources::buffer_write(vertex_buffer, 0, &vertices).map_err(|e| e.text())?;
    let indices: Vec<u8> = [0u16, 1, 2, 0, 2, 3]
        .iter()
        .flat_map(|i| i.to_le_bytes())
        .collect();
    let index_buffer = resources::buffer_create(device, 12, 4).map_err(|e| e.text())?;
    resources::buffer_write(index_buffer, 0, &indices).map_err(|e| e.text())?;
    let texels = [
        [255u8, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
    ];
    let albedo = resources::texture_create(device, &[2, 2, 1, 1, 0, 0, 1]).map_err(|e| e.text())?;
    resources::texture_write(albedo, 0, 0, &texels.concat()).map_err(|e| e.text())?;
    expect_err(
        resources::texture_write(albedo, 0, 0, &[0; 3]),
        Code::Invalid,
        "short texel data",
    );
    expect_err(
        resources::texture_write(albedo, 1, 0, &[0; 4]),
        Code::Invalid,
        "missing mip",
    );
    let nearest =
        resources::sampler_create(device, &[0, 0, 0, 0, 0, 0, 0, 1]).map_err(|e| e.text())?;
    let (size, depth_format) = (8, 10);
    let target =
        resources::texture_create(device, &[size, size, 1, 1, 0, 1, 1]).map_err(|e| e.text())?;
    let depth = resources::texture_create(device, &[size, size, 1, 1, depth_format, 1, 1])
        .map_err(|e| e.text())?;
    let bits = |v: f32| v.to_bits() as i32;
    let mut commands = Commands::default();
    commands
        .op(
            1,
            &[
                target,
                1,
                bits(0.5),
                bits(0.5),
                bits(0.5),
                bits(1.0),
                depth,
                1,
                bits(1.0),
            ],
        )
        .op(4, &[pipeline])
        .op(7, &[0, vertex_buffer, 0])
        .op(8, &[index_buffer, 0, 0])
        .op(
            9,
            &[0, 0, bits(size as f32), bits(size as f32), 0, bits(1.0)],
        )
        .op(5, &[2, 0, 2, albedo, 0])
        .op(5, &[2, 1, 3, nearest, 0])
        .op(12, &[6, 1, 0, 0, 0])
        .op(3, &[]);
    commands.submit(device)?;
    let id = future();
    resources::texture_read(id, target, 0, 0);
    let pixels = unsafe { guest::test_bytes(wait(id)) };
    if pixels.len() != (size * size * 4) as usize {
        return Err(format!(
            "quad readback returned {} bytes: {}",
            pixels.len(),
            error::last_text()
        ));
    }
    let at = |x: i32, y: i32| pixels[((y * size + x) * 4) as usize..][..4].to_vec();
    let mut corners = vec![
        at(0, 0),
        at(size - 1, 0),
        at(0, size - 1),
        at(size - 1, size - 1),
    ];
    corners.sort();
    let mut expected: Vec<Vec<u8>> = texels.iter().map(|t| t.to_vec()).collect();
    expected.sort();
    if corners != expected {
        return Err(format!(
            "quad corners are {corners:?}, expected the four texels"
        ));
    }
    if devices::backend(device).map_err(|e| e.text())? == Backend::Dx12 as i32 {
        // Distinct sampler objects prevent shader binding caches from merging draws.
        let stress_samplers = (0..2200)
            .map(|_| {
                resources::sampler_create(device, &[0, 0, 0, 0, 0, 0, 0, 1]).map_err(|e| e.text())
            })
            .collect::<Result<Vec<_>, _>>()?;
        for draws in [1200, 1200, 1200, 2200, 1200, 1200, 1200, 1200] {
            let mut stress = Commands::default();
            stress.op(1, &[target, 1, 0, 0, 0, bits(1.0), depth, 1, bits(1.0)]);
            for sampler in stress_samplers.iter().take(draws) {
                stress
                    .op(4, &[pipeline])
                    .op(7, &[0, vertex_buffer, 0])
                    .op(8, &[index_buffer, 0, 0])
                    .op(5, &[2, 0, 2, albedo, 0])
                    .op(5, &[2, 1, 3, *sampler, 0])
                    .op(12, &[6, 1, 0, 0, 0]);
            }
            let result = stress.op(3, &[]).submit(device);
            if draws > 2048 {
                if result.is_ok() {
                    return Err("oversized sampler submission unexpectedly succeeded".to_string());
                }
            } else {
                result?;
            }
        }
        for sampler in stress_samplers {
            testing::gpuRelease(sampler);
        }
    }
    for handle in [
        target,
        depth,
        nearest,
        albedo,
        index_buffer,
        vertex_buffer,
        pipeline,
        library,
    ] {
        testing::gpuRelease(handle);
    }
    Ok(())
}

/// Each host failure reaches the guest as the GpuError code it promises.
fn errors(device: i32) -> Result<(), String> {
    let buffer = resources::buffer_create(device, 16, 1).map_err(|e| e.text())?;
    expect_err(
        resources::buffer_create(device, -1, 1),
        Code::Invalid,
        "negative size",
    );
    expect_err(
        resources::buffer_write(buffer, 12, &[0; 8]),
        Code::Invalid,
        "write past the end",
    );
    let id = future();
    resources::buffer_read(id, buffer, 8, 16);
    assert!(
        unsafe { guest::test_bytes(wait(id)) }.is_empty(),
        "read past the end is empty"
    );
    assert_eq!(last_code(), "EINVALID");
    assert!(
        read_buffer(buffer, 0, 0).is_empty(),
        "an empty read of an empty range succeeds"
    );

    expect_err(
        pipelines::library_load(device, b"not a bundle"),
        Code::Invalid,
        "garbage bundle",
    );
    let library = pipelines::library_load(device, SAXPY).map_err(|e| e.text())?;
    expect_err(
        pipelines::compute_pipeline_create(library, "missing"),
        Code::NotCompiled,
        "entry without code for this backend",
    );
    expect_err(
        pipelines::render_pipeline_create(
            library,
            "saxpy",
            library,
            "scale",
            &[0, 0, 0, 0, 0, 0, 0, 1, 0],
        ),
        Code::Validation,
        "compute entries in a render pipeline",
    );
    let pipeline = pipelines::compute_pipeline_create(library, "scale").map_err(|e| e.text())?;

    let mut wrong_kind = Commands::default();
    wrong_kind
        .op(2, &[])
        .op(4, &[pipeline])
        .value(0, 0, &[0; 4])
        .op(3, &[]);
    assert!(wrong_kind.submit(device).is_err());
    assert_eq!(
        last_code(),
        "EVALIDATION",
        "a value bound to a buffer parameter"
    );

    let mut open_pass = Commands::default();
    open_pass.op(2, &[]);
    assert!(open_pass.submit(device).is_err());
    assert_eq!(last_code(), "EVALIDATION", "an unended pass");

    let mut released = Commands::default();
    let gone = resources::buffer_create(device, 16, 1).map_err(|e| e.text())?;
    testing::gpuRelease(gone);
    released
        .op(2, &[])
        .op(4, &[pipeline])
        .buffer(0, 0, gone)
        .op(3, &[]);
    assert!(released.submit(device).is_err());
    assert_eq!(last_code(), "EINVALID", "a released buffer");

    let other = devices::create_now(devices::backend(device).unwrap(), 0).map_err(|e| e.text())?;
    let foreign = resources::buffer_create(other, 16, 1).map_err(|e| e.text())?;
    let mut wrong_device = Commands::default();
    wrong_device
        .op(2, &[])
        .op(4, &[pipeline])
        .buffer(0, 0, foreign)
        .op(3, &[]);
    assert!(wrong_device.submit(device).is_err());
    assert_eq!(last_code(), "EWRONGDEVICE", "another device's buffer");

    expect_err(
        resources::descriptor_handle(pipeline),
        if devices::features(device).unwrap() & (1 << 2) != 0 {
            Code::Invalid
        } else {
            Code::Unsupported
        },
        "a pipeline has no bindless handle",
    );
    for handle in [foreign, other, buffer, pipeline, library] {
        testing::gpuRelease(handle);
    }
    Ok(())
}

fn device_loss(device: i32) -> Result<(), String> {
    let buffer = resources::buffer_create(device, 16, 1).map_err(|e| e.text())?;
    assert!(testing::simulate_device_loss(
        device,
        "simulated driver reset"
    ));
    expect_err(devices::check(device), Code::DeviceLost, "check after loss");
    expect_err(
        resources::buffer_create(device, 16, 1),
        Code::DeviceLost,
        "create after loss",
    );
    let id = future();
    resources::buffer_read(id, buffer, 0, 16);
    assert!(unsafe { guest::test_bytes(wait(id)) }.is_empty());
    assert_eq!(error::last_text(), "EDEVICELOST\nsimulated driver reset");
    let mut commands = Commands::default();
    commands.op(2, &[]).op(3, &[]);
    assert!(commands.submit(device).is_err());
    assert_eq!(last_code(), "EDEVICELOST");
    testing::gpuRelease(buffer);
    testing::gpuRelease(device);
    Ok(())
}

/// A real window from the window host, presented through this host on the main thread.
fn surface(device: i32) -> Result<(), String> {
    use dream_host_window::{dream_window_surface_v1, host};
    testing::set_window_surface_fn(dream_window_surface_v1);
    let window =
        host::open("dream-host-gpu surface", 320, 200, 4).map_err(|e| e.code.to_string())?;
    host::poll(window);
    let surface = surfaces::surface_create(device, window, true)
        .map_err(|e| format!("surface create: {}", e.text()))?;
    let [format, width, height] = surfaces::surface_info(surface).map_err(|e| e.text())?;
    if !(0..=3).contains(&format) || width <= 0 || height <= 0 {
        return Err(format!("surface info [{format}, {width}, {height}]"));
    }
    let bits = |v: f32| v.to_bits() as i32;
    for frame in 0..3 {
        let id = future();
        surfaces::acquire(id, surface);
        let image = wait(id) as i32;
        if image <= 0 {
            return Err(format!("acquire: {}", error::last_text()));
        }
        let mut commands = Commands::default();
        commands
            .op(
                1,
                &[
                    image,
                    1,
                    bits(0.1 * frame as f32),
                    bits(0.2),
                    bits(0.3),
                    bits(1.0),
                    0,
                    0,
                    0,
                ],
            )
            .op(3, &[]);
        commands.submit(device)?;
        let id = future();
        surfaces::present(id, surface);
        if wait(id) != 0 {
            return Err(format!("present: {}", error::last_text()));
        }
        host::poll(window);
    }
    surfaces::surface_resize(surface, 640, 400)
        .map_err(|e| format!("surface resize: {}", e.text()))?;
    let resized = surfaces::surface_info(surface).map_err(|e| e.text())?;
    if resized[1..] != [640, 400] {
        return Err(format!("resized surface reports {resized:?}"));
    }
    expect_err(
        surfaces::surface_resize(surface, 0, 10),
        Code::Invalid,
        "zero-size resize",
    );
    testing::gpuRelease(surface);
    host::close(window);
    expect_err(
        surfaces::surface_create(device, window, true),
        Code::Surface,
        "a closed window",
    );
    Ok(())
}
