//! `system.gpu` shader-side declarations type-check inside shaders the `system.gpu.shaders`
//! generator consumes, and are rejected everywhere else.

use std::process::Command;

const SHADERS: &str = include_str!("cases/support/gpu_api/shaders.dream");
const MAIN: &str = include_str!("cases/support/gpu_api/main.dream");

#[test]
#[ignore = "needs the pinned slang toolchain"]
fn shader_declarations_type_check_inside_consumed_shaders() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dream.toml"),
        "[package]\nname = \"gpu_api\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("shaders.dream"), SHADERS).unwrap();
    std::fs::write(dir.path().join("main.dream"), MAIN).unwrap();
    // A Linux object keeps the Vulkan targets the ray tracing and cooperative-vector shaders
    // need on any build host.
    let build = || {
        Command::new(env!("CARGO_BIN_EXE_dream"))
            .current_dir(dir.path())
            .args([
                "build",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--object",
                "main.dream",
            ])
            .output()
            .unwrap()
    };

    let ok = build();
    let log = String::from_utf8_lossy(&ok.stderr);
    assert!(ok.status.success(), "{log}");

    std::fs::write(
        dir.path().join("main.dream"),
        MAIN.replace(
            "fun main(): void {",
            "fun read(buffer: GpuBuffer<float>, scene: GpuAccelerationStructure): float {\n    let ray = GpuRay.of(GpuVec3.splat(0.0f), GpuVec3.splat(1.0f), 0.0f, 1.0f);\n    return GpuMath.sin(buffer[0]);\n}\n\nfun main(): void {",
        ),
    )
    .unwrap();
    let misuse = build();
    let log = String::from_utf8_lossy(&misuse.stderr);
    assert!(!misuse.status.success(), "{log}");
    for expected in [
        "'of' is only available inside functions marked @compute",
        "'sin' is only available inside functions marked @compute",
        "'GpuBuffer<float>[]' is only available inside functions marked @compute",
    ] {
        assert!(log.contains(expected), "missing `{expected}` in {log}");
    }
}
