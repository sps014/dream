//! The `system.gpu.shaders` generator: Slang translation, `slangc` compilation, bundles and
//! typed `GpuShaders` wrappers, plus the shader rules it enforces.
//!
//! Golden cases compare the readable artifacts (`<module>.slang`, `<module>.bundle.json` and the
//! generated wrapper source) with `cases/support/gpu_shaders/<case>/expected/`. Rewrite them with
//! `DREAM_BLESS_GPU=1` after reviewing the change. Every test needs the pinned Slang toolchain
//! (`dreamer toolchain install slang` or `DREAM_SLANG`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MAIN: &str = include_str!("cases/support/gpu_shaders/main.dream");

fn support(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/cases/support/gpu_shaders")
        .join(case)
}

fn project(shaders: &str, main: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dream.toml"),
        "[package]\nname = \"gpu_shaders\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("shaders.dream"), shaders).unwrap();
    std::fs::write(dir.path().join("main.dream"), main).unwrap();
    dir
}

/// An unlinked Linux object, so the native targets (Vulkan, Dx12) are kept on any build host.
const LINUX: &[&str] = &["--target", "x86_64-unknown-linux-gnu", "--object"];
/// An unlinked Apple object, which keeps the Metal target.
const APPLE: &[&str] = &["--target", "aarch64-apple-darwin", "--object"];

/// Builds `main.dream`: a wasm32 module, or a native Linux object.
fn build(dir: &Path, native: bool) -> Output {
    build_with(dir, if native { LINUX } else { &["--wasm"] })
}

fn build_with(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dream"))
        .current_dir(dir)
        .arg("build")
        .args(args)
        .arg("main.dream")
        .output()
        .unwrap()
}

fn generated(dir: &Path) -> PathBuf {
    dir.join(".dream/generated/main/gpu_shaders")
}

fn log(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn golden(case: &str, native: bool) {
    let source = std::fs::read_to_string(support(case).join("shaders.dream")).unwrap();
    let dir = project(&source, MAIN);
    let output = build(dir.path(), native);
    assert!(output.status.success(), "{}", log(&output));
    let out = generated(dir.path());
    let expected = support(case).join("expected");
    let bless = std::env::var_os("DREAM_BLESS_GPU").is_some();
    for (file, golden) in [
        ("shaders.slang", "shaders.slang.golden"),
        ("shaders.bundle.json", "bundle.json.golden"),
        ("shaders.gpu.dream", "wrappers.dream.golden"),
    ] {
        let actual = std::fs::read_to_string(out.join(file))
            .unwrap_or_else(|e| panic!("{file}: {e}\n{}", log(&output)));
        let path = expected.join(golden);
        if bless {
            std::fs::create_dir_all(&expected).unwrap();
            std::fs::write(&path, &actual).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (bless with DREAM_BLESS_GPU=1)", path.display()));
        if native && file == "shaders.bundle.json" {
            // Slang's SPIR-V ID assignment can differ across host platforms. Keep layouts,
            // sizes and bindings golden; two_builds_are_byte_identical checks the raw bytes.
            let mut actual: serde_json::Value = serde_json::from_str(&actual).unwrap();
            let mut want: serde_json::Value = serde_json::from_str(&want).unwrap();
            without_blob_hashes(&mut actual);
            without_blob_hashes(&mut want);
            assert_eq!(actual, want, "{case}/{golden} differs");
        } else {
            assert_eq!(actual, want, "{case}/{golden} differs");
        }
    }
    let bundle = std::fs::read(out.join("shaders.bundle")).unwrap();
    assert_eq!(&bundle[..4], b"DGPB");
}

fn without_blob_hashes(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            fields.remove("sha256");
            for child in fields.values_mut() {
                without_blob_hashes(child);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                without_blob_hashes(child);
            }
        }
        _ => {}
    }
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn compute_saxpy_golden() {
    golden("compute", false);
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn workgroup_reduction_and_atomics_golden() {
    golden("reduction", false);
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn vertex_fragment_triangle_golden() {
    golden("triangle", false);
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn ray_tracing_stages_and_ray_query_golden() {
    golden("raytracing", true);
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn autodiff_golden() {
    golden("autodiff", false);
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn subgroups_golden() {
    golden("subgroups", false);
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn bindless_golden() {
    golden("bindless", true);
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn cooperative_vectors_and_matrices_golden() {
    golden("neural", true);
}

/// A web build with two shader modules: both bundles go through the browser host
/// (`scripts/test-gpu-host.mjs` with a fake `navigator.gpu`), which checks entry names, bind
/// group layouts and uniform offsets against the WGSL, and the selective runtime carries the
/// GPU host.
#[test]
#[ignore = "needs the pinned slang toolchain and Node"]
fn web_bundles_drive_the_browser_host() {
    let triangle = std::fs::read_to_string(support("triangle").join("shaders.dream"))
        .unwrap()
        .replace("module shaders;", "module triangle;");
    let dir = project(
        &std::fs::read_to_string(support("compute").join("shaders.dream")).unwrap(),
        "import system;\nimport system.gpu;\nimport shaders;\nimport triangle;\n\nasync fun main(): void {\n    switch (GpuDevice.create(GpuBackend.Auto).await) {\n        Ok(device) => {\n            System.println(device.backend == GpuBackend.WebGpu);\n        }\n        Err(e) => {\n            System.println(e.message());\n        }\n    }\n}\n",
    );
    std::fs::write(dir.path().join("triangle.dream"), triangle).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .current_dir(dir.path())
        .args(["build", "--web", "main.dream"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", log(&output));
    let runtime =
        std::fs::read_to_string(dir.path().join("target/web/main.web.runtime.js")).unwrap();
    assert!(
        runtime.contains("gpuDeviceCreate"),
        "the web runtime lacks the GPU host"
    );
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/test-gpu-host.mjs");
    let node = Command::new("node")
        .arg(script)
        .env("DREAM_GPU_BUNDLES", generated(dir.path()))
        .output()
        .unwrap();
    let text = log(&node);
    assert!(node.status.success(), "{text}");
    assert!(
        text.contains("✔ generator bundles translate into WebGPU pipelines"),
        "{text}"
    );
}

/// Every artifact of two builds of the same source is byte-identical.
#[test]
#[ignore = "needs the pinned slang toolchain"]
fn two_builds_are_byte_identical() {
    builds_are_identical("triangle", false);
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn native_shader_bundles_are_byte_identical() {
    builds_are_identical("neural", true);
}

fn builds_are_identical(case: &str, native: bool) {
    let source = std::fs::read_to_string(support(case).join("shaders.dream")).unwrap();
    let mut runs = Vec::new();
    for _ in 0..2 {
        let dir = project(&source, MAIN);
        let output = build(dir.path(), native);
        assert!(output.status.success(), "{}", log(&output));
        let out = generated(dir.path());
        let files: Vec<(String, Vec<u8>)> = [
            "shaders.slang",
            "shaders.bundle",
            "shaders.bundle.json",
            "shaders.gpu.dream",
        ]
        .iter()
        .map(|f| (f.to_string(), std::fs::read(out.join(f)).unwrap()))
        .collect();
        runs.push(files);
    }
    assert_eq!(runs[0], runs[1]);
}

/// Builds a failing program and checks each expected message and its `shaders.dream` line.
fn rejects(shaders: &str, native: bool, expected: &[(&str, usize)]) {
    rejects_with(shaders, if native { LINUX } else { &["--wasm"] }, expected);
}

fn rejects_with(shaders: &str, args: &[&str], expected: &[(&str, usize)]) {
    let dir = project(shaders, MAIN);
    let output = build_with(dir.path(), args);
    let text = log(&output);
    assert!(!output.status.success(), "{text}");
    reports(&text, expected);
}

/// Checks that each expected message appears in `text` at its `shaders.dream` line.
fn reports(text: &str, expected: &[(&str, usize)]) {
    for (message, line) in expected {
        assert!(text.contains(message), "missing `{message}` in {text}");
        let location = format!("shaders.dream:{line}:");
        assert!(
            text.lines()
                .skip_while(|l| !l.contains(message))
                .take(3)
                .any(|l| l.contains(&location)),
            "`{message}` is not reported at {location} in {text}"
        );
    }
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn recursion_is_rejected_at_the_call() {
    rejects(
        "module shaders;\nimport system.gpu;\n\n@gpu\nfun depth(n: int): int {\n    if n <= 0 {\n        return 0;\n    }\n    return depth(n - 1) + 1;\n}\n\n@compute(1)\npublic fun run(out: GpuBuffer<int>): void {\n    out[0] = depth(4);\n}\n",
        false,
        &[(
            "recursion is not allowed in GPU shaders ('depth' calls itself)",
            9,
        )],
    );
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn heap_classes_strings_and_closures_are_rejected() {
    rejects(
        "module shaders;\nimport system.gpu;\n\nclass Counter {\n    public value: int;\n}\n\n@compute(1)\npublic fun run(out: GpuBuffer<int>): void {\n    let counter = Counter();\n    let label = \"total\";\n    let twice = (v: int) => v * 2;\n    out[0] = 1;\n}\n",
        false,
        &[
            (
                "'Counter' is not a struct or enum; GPU shaders cannot use classes, unions or interfaces",
                10,
            ),
            ("strings are not allowed in GPU shaders", 11),
            ("closures are not allowed in GPU shaders", 12),
        ],
    );
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn an_unknown_builtin_does_not_cascade_into_its_siblings() {
    let dir = project(
        "module shaders;\nimport system.gpu;\n\n@vertex\npublic fun corner(): GpuVec4 {\n    let x = GpuMath.radians(1.0f) * (float)vertex_id();\n    return GpuVec4.of(x, 0.0f, 0.0f, 1.0f);\n}\n",
        MAIN,
    );
    let output = build_with(dir.path(), &["--wasm"]);
    let text = log(&output);
    assert!(!output.status.success(), "{text}");
    reports(&text, &[("'radians' is not available in GPU shaders", 6)]);
    assert!(!text.contains("'vertex_id' is a CPU function"), "{text}");
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn slang_reserved_words_are_renamed_in_helpers_and_rejected_as_shader_names() {
    rejects(
        "module shaders;\nimport system.gpu;\n\n@gpu\nfun inline(layout: int): int {\n    let each = layout + 1;\n    return each;\n}\n\n@compute(1)\npublic fun unused(out: GpuBuffer<int>): void {\n    out[0] = inline(1);\n}\n",
        false,
        &[(
            "'unused' is a reserved word in the Slang shader language; rename this shader",
            11,
        )],
    );
    let dir = project(
        "module shaders;\nimport system.gpu;\n\n@gpu\nfun inline(layout: int): int {\n    let each = layout + 1;\n    return each;\n}\n\n@compute(1)\npublic fun run(out: GpuBuffer<int>): void {\n    out[0] = inline(1);\n}\n",
        MAIN,
    );
    let output = build_with(dir.path(), &["--wasm"]);
    assert!(output.status.success(), "{}", log(&output));
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn unsupported_types_and_labeled_break_are_rejected() {
    rejects(
        "module shaders;\nimport system.gpu;\n\n@compute(1)\npublic fun run(out: GpuBuffer<int>): void {\n    let wide: usize = 1;\n    outer: while true {\n        break outer;\n    }\n    out[0] = 1;\n}\n",
        false,
        &[
            ("type 'usize' is not supported in GPU shaders", 6),
            ("labeled statements are not supported in GPU shaders", 7),
        ],
    );
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn cooperative_vectors_need_a_target_that_has_them() {
    rejects(
        "module shaders;\nimport system.gpu;\n\n@compute(32)\npublic fun run(@readonly @coop_shape(16, 4) layer: GpuMlpWeights<float16>, @readonly @coop_shape(16) input: GpuBuffer<GpuCoopVec<float16>>, @coop_shape(4) out: GpuBuffer<GpuCoopVec<float16>>): void {\n    let i = global_id().x;\n    out[i] = layer.apply(input[i]);\n}\n",
        false,
        &[(
            "cooperative vectors needs GpuTarget.Dx12, Vulkan or Cuda; add one to @gpu_targets",
            5,
        )],
    );
}

/// Metal and WebGPU have no cooperative vectors and WebGPU no cooperative matrices (Slang would
/// stop with E36107); the generator reports them at their first use instead. Metal's
/// cooperative matrices are 8 by 8.
#[test]
#[ignore = "needs the pinned slang toolchain"]
fn cooperative_types_on_metal_and_webgpu_are_rejected_at_first_use() {
    rejects_with(
        "module shaders;\nimport system.gpu;\n\n@gpu_targets(GpuTarget.Metal, GpuTarget.WebGpu)\n@compute(32)\npublic fun layer(@readonly @coop_shape(16, 4) weights: GpuMlpWeights<float16>, @readonly @coop_shape(16) input: GpuBuffer<GpuCoopVec<float16>>, @coop_shape(4) out: GpuBuffer<GpuCoopVec<float16>>): void {\n    let i = global_id().x;\n    out[i] = weights.apply(input[i]);\n}\n\n@gpu_targets(GpuTarget.Metal, GpuTarget.Vulkan)\n@compute(32)\npublic fun tile(@readonly a: GpuBuffer<float16>, c: GpuBuffer<float16>): void {\n    let m = GpuCoopMat.load(a, 0, 16, 16, 16);\n    m.store(c, 0, 16);\n}\n\n@gpu_targets(GpuTarget.WebGpu)\n@compute(32)\npublic fun web_tile(c: GpuBuffer<float16>): void {\n    let m = GpuCoopMat<float16>.zeros(8, 8);\n    m.store(c, 0, 8);\n}\n",
        APPLE,
        &[
            (
                "cooperative vectors needs GpuTarget.Dx12, Vulkan or Cuda; add one to @gpu_targets",
                6,
            ),
            (
                "cooperative matrices are 8 by 8 on GpuTarget.Metal; use 8 by 8 tiles or give 'tile' its own @gpu_targets without GpuTarget.Metal",
                14,
            ),
            (
                "cooperative matrices needs GpuTarget.Vulkan, Metal or Cuda; add one to @gpu_targets",
                21,
            ),
        ],
    );
}

/// WebGPU guarantees 8 storage buffers per stage, counted per pipeline: each compute shader on
/// its own, and natively a module's render shaders together, because they share one layout.
#[test]
#[ignore = "needs the pinned slang toolchain"]
fn storage_buffers_past_the_webgpu_limit_warn_per_pipeline() {
    let separate = project(
        "module shaders;\nimport system.gpu;\n\n@compute(64)\npublic fun first(@readonly a: GpuBuffer<float>, @readonly b: GpuBuffer<float>, @readonly c: GpuBuffer<float>, out: GpuBuffer<float>): void {\n    let i = global_id().x;\n    out[i] = a[i] + b[i] + c[i];\n}\n\n@compute(64)\npublic fun second(@readonly a: GpuBuffer<float>, @readonly b: GpuBuffer<float>, @readonly c: GpuBuffer<float>, @readonly d: GpuBuffer<float>, out: GpuBuffer<float>): void {\n    let i = global_id().x;\n    out[i] = a[i] + b[i] + c[i] + d[i];\n}\n",
        MAIN,
    );
    let output = build(separate.path(), true);
    assert!(output.status.success(), "{}", log(&output));
    assert!(!log(&output).contains("storage buffers"), "{}", log(&output));

    let many = project(
        "module shaders;\nimport system.gpu;\n\n@compute(64)\npublic fun many(@readonly a: GpuBuffer<float>, @readonly b: GpuBuffer<float>, @readonly c: GpuBuffer<float>, @readonly d: GpuBuffer<float>, @readonly e: GpuBuffer<float>, @readonly f: GpuBuffer<float>, @readonly g: GpuBuffer<float>, @readonly h: GpuBuffer<float>, out: GpuBuffer<float>): void {\n    let i = global_id().x;\n    out[i] = a[i] + b[i] + c[i] + d[i] + e[i] + f[i] + g[i] + h[i];\n}\n",
        MAIN,
    );
    for native in [true, false] {
        let output = build(many.path(), native);
        let text = log(&output);
        assert!(output.status.success(), "{text}");
        reports(
            &text,
            &[(
                "'many' binds 9 storage buffers, more than the 8 per stage WebGPU guarantees",
                5,
            )],
        );
    }

    let render = project(
        "module shaders;\nimport system.gpu;\n\n@vertex\npublic fun corner(@readonly a: GpuBuffer<float>, @readonly b: GpuBuffer<float>, @readonly c: GpuBuffer<float>, @readonly d: GpuBuffer<float>): GpuVec4 {\n    let i = (int)vertex_id();\n    return GpuVec4.of(a[i], b[i], c[i] + d[i], 1.0f);\n}\n\n@fragment\npublic fun paint(@readonly a: GpuBuffer<float>, @readonly b: GpuBuffer<float>, @readonly c: GpuBuffer<float>, @readonly d: GpuBuffer<float>, @readonly e: GpuBuffer<float>): GpuVec4 {\n    return GpuVec4.of(a[0], b[0], c[0], d[0] + e[0]);\n}\n",
        MAIN,
    );
    let output = build(render.path(), true);
    let text = log(&output);
    assert!(output.status.success(), "{text}");
    reports(
        &text,
        &[(
            "the vertex and fragment shaders of this module bind 9 storage buffers together",
            11,
        )],
    );
    let web = build(render.path(), false);
    assert!(web.status.success(), "{}", log(&web));
    assert!(!log(&web).contains("storage buffers"), "{}", log(&web));
}

/// A shader limited to targets the build cannot use is left out with a warning, so one program
/// can carry platform-specific shaders, even when that leaves a module with no shader at all.
#[test]
#[ignore = "needs the pinned slang toolchain"]
fn shaders_for_other_platforms_are_left_out_with_a_warning() {
    let dir = project(
        "module shaders;\nimport system.gpu;\n\n@compute(64)\npublic fun twice(data: GpuBuffer<float>): void {\n    let i = global_id().x;\n    data[i] = data[i] * 2.0f;\n}\n\n@gpu_targets(GpuTarget.Metal)\n@compute(64)\npublic fun halve(data: GpuBuffer<float>): void {\n    let i = global_id().x;\n    data[i] = data[i] * 0.5f;\n}\n",
        MAIN,
    );
    let output = build(dir.path(), true);
    let text = log(&output);
    assert!(output.status.success(), "{text}");
    reports(
        &text,
        &[(
            "'halve' has no @gpu_targets that can run on Linux, so this build leaves it out",
            12,
        )],
    );
    let apple = build_with(dir.path(), APPLE);
    assert!(apple.status.success(), "{}", log(&apple));
    assert!(!log(&apple).contains("leaves it out"), "{}", log(&apple));

    let dir = project(
        "@gpu_targets(GpuTarget.Metal)\nmodule shaders;\nimport system.gpu;\n\n@compute(64)\npublic fun halve(data: GpuBuffer<float>): void {\n    let i = global_id().x;\n    data[i] = data[i] * 0.5f;\n}\n",
        MAIN,
    );
    let web = build(dir.path(), false);
    let text = log(&web);
    assert!(web.status.success(), "{text}");
    reports(
        &text,
        &[(
            "'halve' has no @gpu_targets that can run on wasm32, so this build leaves it out",
            2,
        )],
    );
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn cuda_render_stages_and_unusable_targets_are_rejected() {
    rejects(
        "@gpu_targets(GpuTarget.Cuda, GpuTarget.Vulkan)\nmodule shaders;\nimport system.gpu;\n\n@vertex\npublic fun corner(): GpuVec4 {\n    return GpuVec4.of(0.0f, 0.0f, 0.0f, 1.0f);\n}\n",
        false,
        &[
            (
                "GpuTarget.Cuda runs compute shaders only; give 'corner' its own @gpu_targets without GpuTarget.Cuda",
                6,
            ),
            (
                "'corner' has no @gpu_targets that can run on wasm32, so this build leaves it out",
                2,
            ),
        ],
    );
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn ray_tracing_needs_a_ray_tracing_target() {
    rejects(
        "module shaders;\nimport system.gpu;\n\npublic struct Payload {\n    public value: float;\n}\n\n@miss\npublic fun sky(ref payload: Payload): void {\n    payload.value = 1.0f;\n}\n",
        false,
        &[(
            "ray tracing needs GpuTarget.Dx12 or Vulkan; add one to @gpu_targets",
            9,
        )],
    );
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn referencing_a_builtin_without_calling_it_says_to_call_it() {
    rejects(
        "module shaders;\nimport system.gpu;\n\n@compute(64)\npublic fun fill(out: GpuBuffer<int>): void {\n    let i = global_id.x;\n    out[i] = i;\n}\n",
        false,
        &[("'global_id' is a function; call it as 'global_id()'", 6)],
    );
}

/// Targets whose downstream compiler Slang cannot load on this machine are build errors at
/// the shader, not crashes. macOS has neither dxc nor NVRTC; other hosts may have them.
#[test]
#[ignore = "needs the pinned slang toolchain"]
fn missing_downstream_compilers_are_build_errors() {
    if !cfg!(target_os = "macos") {
        return;
    }
    let dir = project(
        "module shaders;\nimport system.gpu;\n\n@compute(64)\n@gpu_targets(GpuTarget.Dx12, GpuTarget.Vulkan)\npublic fun twice(out: GpuBuffer<float>): void {\n    let i = global_id().x;\n    out[i] = 2.0f * out[i];\n}\n",
        MAIN,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .current_dir(dir.path())
        .args([
            "build",
            "--target",
            "x86_64-pc-windows-msvc",
            "--object",
            "main.dream",
        ])
        .output()
        .unwrap();
    let text = log(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("GpuTarget.Dx12 needs the DirectX Shader Compiler (dxc), which Slang could not load on this build machine")
            && text.contains("shaders.dream:6:"),
        "{text}"
    );
    assert!(
        !text.contains("panicked") && !text.contains("internal compiler error"),
        "{text}"
    );

    std::fs::write(
        dir.path().join("shaders.dream"),
        "module shaders;\nimport system.gpu;\n\n@compute(64)\n@gpu_targets(GpuTarget.Cuda)\npublic fun twice(out: GpuBuffer<float>): void {\n    let i = global_id().x;\n    out[i] = 2.0f * out[i];\n}\n",
    )
    .unwrap();
    let output = build(dir.path(), true);
    let text = log(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("GpuTarget.Cuda needs NVRTC from the CUDA toolkit")
            && text.contains("shaders.dream:6:"),
        "{text}"
    );
    assert!(
        !text.contains("panicked") && !text.contains("internal compiler error"),
        "{text}"
    );
}

/// A Slang compile error is reported at the Dream line whose translation Slang rejected.
#[test]
#[ignore = "needs the pinned slang toolchain"]
fn slang_errors_map_back_to_the_dream_line() {
    rejects(
        "module shaders;\nimport system.gpu;\n\n@compute(64)\npublic fun scale(@readonly values: GpuBuffer<float>, factor: float): void {\n    let i = global_id().x;\n    values[i] = values[i] * factor;\n}\n",
        false,
        &[("(slangc, target wgsl)", 7)],
    );
}

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn consumed_shaders_point_at_their_generated_entry() {
    let dir = project(
        &std::fs::read_to_string(support("compute").join("shaders.dream")).unwrap(),
        "import system;\nimport system.gpu;\nimport shaders;\n\nfun run(buffer: GpuBuffer<float>): void {\n    saxpy(buffer, buffer, buffer, 1.0f, 4);\n}\n\nfun main(): void {}\n",
    );
    let output = build(dir.path(), false);
    let text = log(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("'saxpy' was consumed by generator 'gpu_shaders'")
            && text.contains("dispatch it with GpuShaders.saxpy.dispatch(device, ...)"),
        "{text}"
    );
}
