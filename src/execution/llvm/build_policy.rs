use crate::driver::wasm_opt::OptLevel;

/// Backend tuning is independent of whether source debug information is emitted.
pub(crate) fn pipeline(opt: OptLevel) -> &'static str {
    match opt {
        OptLevel::O0 => "internalize,default<O0>",
        OptLevel::O1 => "internalize,default<O1>",
        OptLevel::O2 => "internalize,default<O2>",
        OptLevel::O3 | OptLevel::O4 => "internalize,default<O3>",
        OptLevel::Size => "internalize,default<Os>",
        OptLevel::SizeAggressive => "internalize,default<Oz>",
    }
}

pub(crate) fn llc_level(opt: OptLevel) -> &'static str {
    match opt {
        OptLevel::O0 => "-O0",
        OptLevel::O1 => "-O1",
        OptLevel::O2 | OptLevel::Size | OptLevel::SizeAggressive => "-O2",
        OptLevel::O3 | OptLevel::O4 => "-O3",
    }
}

/// The CPU for the program and its runtime (whose bitcode carries none): the host's own at
/// `-O3`/`-O4`, where the binary is built to run here; otherwise the target's baseline, which on
/// Apple silicon is the M1 every arm64 Mac has.
pub(crate) fn cpu_args(
    opt: OptLevel,
    spec: &dream_abi::target::TargetSpec,
) -> &'static [&'static str] {
    if spec.can_link_on_host() && matches!(opt, OptLevel::O3 | OptLevel::O4) {
        &["-mcpu=native"]
    } else if spec.is_apple() && !spec.is_ios() && spec.triple.architecture.to_string() == "aarch64"
    {
        &["-mcpu=apple-m1"]
    } else {
        &[]
    }
}

pub(super) fn section_args(spec: &dream_abi::target::TargetSpec) -> &'static [&'static str] {
    if !spec.is_windows() && !spec.is_apple() {
        &["-function-sections", "-data-sections"]
    } else {
        &[]
    }
}

pub(super) fn dead_strip_args(spec: &dream_abi::target::TargetSpec) -> &'static [&'static str] {
    if spec.is_apple() {
        &["-Wl,-dead_strip"]
    } else if !spec.is_windows() {
        &["-Wl,--gc-sections"]
    } else {
        &[]
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn native_dead_stripping_matches_object_section_policy() {
        let spec = &dream_abi::target::TargetSpec::host();
        if cfg!(target_os = "linux") {
            assert_eq!(
                section_args(spec),
                &["-function-sections", "-data-sections"]
            );
            assert_eq!(dead_strip_args(spec), &["-Wl,--gc-sections"]);
        } else if cfg!(target_os = "macos") {
            assert!(section_args(spec).is_empty());
            assert_eq!(dead_strip_args(spec), &["-Wl,-dead_strip"]);
        } else {
            assert!(section_args(spec).is_empty());
            assert!(dead_strip_args(spec).is_empty());
        }
    }
    #[test]
    fn foreign_targets_use_their_own_section_and_cpu_policy() {
        for triple in [
            "x86_64-unknown-linux-gnu",
            "aarch64-apple-darwin",
            "aarch64-pc-windows-msvc",
        ] {
            let spec = dream_abi::target::TargetSpec::parse(triple).unwrap();
            assert_eq!(
                section_args(&spec).is_empty(),
                spec.is_windows() || spec.is_apple()
            );
            if !spec.can_link_on_host() {
                assert!(!cpu_args(OptLevel::O3, &spec).contains(&"-mcpu=native"));
            }
        }
    }
}
