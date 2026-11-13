use super::StdPackage;
use dream_abi::toolchain::{ArchiveKind, HostArchive, OverrideKind, PinnedTool};

/// Slang shader compiler release the generator compiles shaders with.
const SLANG_VERSION: &str = "2026.20";

const SLANG: PinnedTool = PinnedTool {
    name: "slang",
    aliases: &[],
    version: SLANG_VERSION,
    // `dream-slang-layout` is not in the release archive: `dreamer toolchain install slang`
    // builds it into the install root (tooling/dreamer/src/toolchain/slang_layout.rs).
    executables: &["bin/slangc", "bin/dream-slang-layout"],
    env: "DREAM_SLANG",
    override_kind: OverrideKind::Root,
    bundled: None,
    timeout_secs: 120,
    default_install: false,
    archives: &[
        HostArchive {
            host: "linux-x64",
            url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-linux-x86_64.tar.gz",
            sha256: "e863e568c204810d6e8bc31852f056aef6763c55ea9925db2db7affa060347c9",
            kind: ArchiveKind::TarGz,
        },
        HostArchive {
            host: "linux-arm64",
            url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-linux-aarch64.tar.gz",
            sha256: "b0345743e2153d8c2bd33f56fb5696bcef63fc03513f054a1ae1b8d3dd0152a9",
            kind: ArchiveKind::TarGz,
        },
        HostArchive {
            host: "macos-x64",
            url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-macos-x86_64.tar.gz",
            sha256: "b4f01f615dc625f60c593d7d0a4bbd38e05dfa2a34bfd3973017c309d866feaa",
            kind: ArchiveKind::TarGz,
        },
        HostArchive {
            host: "macos-arm64",
            url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-macos-aarch64.tar.gz",
            sha256: "07fa8833fdbb343c3674c84fdd885910f495f3daaaa0c732b32980151e8b58c4",
            kind: ArchiveKind::TarGz,
        },
        HostArchive {
            host: "windows-x64",
            url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-windows-x86_64.tar.gz",
            sha256: "7b9bb838b105de0bb86390196ac5bcb02efc610c0542517204105cf87ec98399",
            kind: ArchiveKind::TarGz,
        },
        HostArchive {
            host: "windows-arm64",
            url: "https://github.com/shader-slang/slang/releases/download/v2026.20/slang-2026.20-windows-aarch64.tar.gz",
            sha256: "b5802a24e810b2b82e18cc796166692b4431431b66a65b108d111b668a920e7c",
            kind: ArchiveKind::TarGz,
        },
    ],
    install_hint: "run `dreamer toolchain install slang` or set DREAM_SLANG",
};

/// The `system.gpu.shaders` generator: translates `system.gpu` shaders to Slang, compiles them
/// with the pinned `slangc`, and emits shader bundles plus typed `GpuShaders` wrappers.
/// Registered through `system.gpu`'s `generators`; user programs never compile it.
pub(super) const PACKAGE: StdPackage = StdPackage {
    name: "system.gpu.shaders",
    deps: &[
        "system.core",
        "system.primitives",
        "system.collections",
        "system.text",
        "system",
        "system.json",
        "system.encoding",
        "system.crypto",
        "system.codegen",
        "system.gpu",
    ],
    core_language: false,
    web_assets: None,
    tools: &[SLANG],
    guest_exports: &[],
    generators: &[],
    files: &[
        (
            "<std>/system/gpu/shaders/web_gpu_binding_layout.dream",
            include_str!("../system/gpu/shaders/web_gpu_binding_layout.dream"),
        ),
        (
            "<std>/system/gpu/shaders/web_gpu_layout.dream",
            include_str!("../system/gpu/shaders/web_gpu_layout.dream"),
        ),
        (
            "<std>/system/gpu/shaders/slang_emit_constructors.dream",
            include_str!("../system/gpu/shaders/slang_emit_constructors.dream"),
        ),
        (
            "<std>/system/gpu/shaders/builtins.dream",
            include_str!("../system/gpu/shaders/builtins.dream"),
        ),
        (
            "<std>/system/gpu/shaders/bundle.dream",
            include_str!("../system/gpu/shaders/bundle.dream"),
        ),
        (
            "<std>/system/gpu/shaders/bundle_target.dream",
            include_str!("../system/gpu/shaders/bundle_target.dream"),
        ),
        (
            "<std>/system/gpu/shaders/capabilities.dream",
            include_str!("../system/gpu/shaders/capabilities.dream"),
        ),
        (
            "<std>/system/gpu/shaders/gpu_shaders.dream",
            include_str!("../system/gpu/shaders/gpu_shaders.dream"),
        ),
        (
            "<std>/system/gpu/shaders/names.dream",
            include_str!("../system/gpu/shaders/names.dream"),
        ),
        (
            "<std>/system/gpu/shaders/param_kinds.dream",
            include_str!("../system/gpu/shaders/param_kinds.dream"),
        ),
        (
            "<std>/system/gpu/shaders/reflection.dream",
            include_str!("../system/gpu/shaders/reflection.dream"),
        ),
        (
            "<std>/system/gpu/shaders/shader_analysis.dream",
            include_str!("../system/gpu/shaders/shader_analysis.dream"),
        ),
        (
            "<std>/system/gpu/shaders/shader_entry.dream",
            include_str!("../system/gpu/shaders/shader_entry.dream"),
        ),
        (
            "<std>/system/gpu/shaders/shader_module.dream",
            include_str!("../system/gpu/shaders/shader_module.dream"),
        ),
        (
            "<std>/system/gpu/shaders/shader_node.dream",
            include_str!("../system/gpu/shaders/shader_node.dream"),
        ),
        (
            "<std>/system/gpu/shaders/shader_param.dream",
            include_str!("../system/gpu/shaders/shader_param.dream"),
        ),
        (
            "<std>/system/gpu/shaders/shader_program.dream",
            include_str!("../system/gpu/shaders/shader_program.dream"),
        ),
        (
            "<std>/system/gpu/shaders/slang_body.dream",
            include_str!("../system/gpu/shaders/slang_body.dream"),
        ),
        (
            "<std>/system/gpu/shaders/slang_body_expr.dream",
            include_str!("../system/gpu/shaders/slang_body_expr.dream"),
        ),
        (
            "<std>/system/gpu/shaders/slang_calls.dream",
            include_str!("../system/gpu/shaders/slang_calls.dream"),
        ),
        (
            "<std>/system/gpu/shaders/slang_diag.dream",
            include_str!("../system/gpu/shaders/slang_diag.dream"),
        ),
        (
            "<std>/system/gpu/shaders/slang_emit.dream",
            include_str!("../system/gpu/shaders/slang_emit.dream"),
        ),
        (
            "<std>/system/gpu/shaders/slang_types.dream",
            include_str!("../system/gpu/shaders/slang_types.dream"),
        ),
        (
            "<std>/system/gpu/shaders/slang_writer.dream",
            include_str!("../system/gpu/shaders/slang_writer.dream"),
        ),
        (
            "<std>/system/gpu/shaders/targets.dream",
            include_str!("../system/gpu/shaders/targets.dream"),
        ),
        (
            "<std>/system/gpu/shaders/verify.dream",
            include_str!("../system/gpu/shaders/verify.dream"),
        ),
        (
            "<std>/system/gpu/shaders/wrappers.dream",
            include_str!("../system/gpu/shaders/wrappers.dream"),
        ),
    ],
};
