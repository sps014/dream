use super::{Cc, MISSING_CXX, TargetSpec, ToolchainConfig};
use std::process::Command;

pub(super) fn command(
    cc: &Cc,
    config: &ToolchainConfig,
    spec: &TargetSpec,
    cxx: bool,
) -> Result<Command, String> {
    if !spec.is_windows() && !spec.is_apple() && spec.os.to_string() != "linux" {
        return Err(format!("native linking is unsupported for {}", spec.triple));
    }
    let mut command = match cc {
        Cc::Zig(path) => {
            let mut command = Command::new(path);
            command.arg(if cxx { "c++" } else { "cc" });
            let os = if spec.is_windows() {
                "windows"
            } else if spec.is_ios() {
                "ios"
            } else if spec.is_apple() {
                "macos"
            } else {
                "linux"
            };
            let abi = if spec.is_msvc() {
                "msvc"
            } else if spec.is_android() {
                "android"
            } else if spec.is_ios() && spec.triple.to_string().ends_with("-sim") {
                "simulator"
            } else if spec.is_apple() {
                "none"
            } else {
                ""
            };
            let abi = if abi.is_empty() {
                spec.env.to_string()
            } else {
                abi.to_string()
            };
            let os = match spec.min_os {
                Some(version) => {
                    format!("{os}.{}.{}.{}", version.major, version.minor, version.patch)
                }
                None => os.to_string(),
            };
            command.args([
                "-target",
                &format!("{}-{os}-{abi}", spec.triple.architecture),
            ]);
            command
        }
        Cc::Program(path) => {
            let path = if cxx {
                config
                    .program(config.cxx.as_ref())
                    .or_else(|| config.find_on_path("clang++"))
                    .or_else(|| config.find_on_path("c++"))
                    .ok_or_else(|| MISSING_CXX.to_string())?
            } else {
                path.clone()
            };
            let mut command = Command::new(path);
            if !spec.can_link_on_host() {
                command.arg(format!("--target={}", spec.llvm_triple()));
            }
            command
        }
    };
    command.args(crate::execution::llvm::runtime::sysroot_args(config, spec));
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zig_commands_preserve_target_and_apple_deployment_version() {
        let config = ToolchainConfig::default();
        let driver = Cc::Zig("zig".into());
        for (triple, expected) in [
            ("aarch64-unknown-linux-gnu", "aarch64-linux-gnu"),
            ("aarch64-pc-windows-msvc", "aarch64-windows-msvc"),
            ("arm64-apple-ios-simulator", "aarch64-ios.13.0.0-simulator"),
        ] {
            let spec = TargetSpec::parse(triple).unwrap();
            let command = command(&driver, &config, &spec, false).unwrap();
            assert!(command.get_args().any(|arg| arg == expected));
        }
    }
}
