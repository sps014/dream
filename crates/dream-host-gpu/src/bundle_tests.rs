use super::write::{EntrySpec, TargetSpec};
use super::*;
use proptest::prelude::*;

fn param(name: &str, kind: ArgKind, binding: u32) -> Param {
    Param {
        name: name.into(),
        webgpu_layout: 0,
        kind,
        flags: if kind == ArgKind::Value {
            0
        } else {
            FLAG_READ_ONLY
        },
        space: 0,
        binding,
        offset: if kind == ArgKind::Value { 4 } else { 0 },
        size: if kind == ArgKind::Value { 4 } else { 0 },
    }
}

fn sample() -> Vec<u8> {
    let shared = b"shared code".as_slice();
    write::bundle(&[
        TargetSpec {
            name: "metal",
            code_format: "msl",
            layouts: vec![b"saxpy layout", b"copy layout"],
            entries: vec![
                EntrySpec {
                    name: "saxpy",
                    stage: "compute",
                    layout: 0,
                    code: b"kernel void saxpy() {}",
                    thread_group_size: [64, 1, 1],
                    params: vec![
                        param("x", ArgKind::Buffer, 0),
                        param("n", ArgKind::Value, 3),
                    ],
                    capabilities: vec!["subgroup_basic"],
                },
                EntrySpec {
                    name: "copy",
                    stage: "compute",
                    layout: 1,
                    code: shared,
                    thread_group_size: [1, 1, 1],
                    params: vec![],
                    capabilities: vec![],
                },
            ],
        },
        TargetSpec {
            name: "wgsl",
            code_format: "wgsl",
            layouts: vec![],
            entries: vec![EntrySpec {
                name: "copy",
                stage: "compute",
                layout: 0,
                code: shared,
                thread_group_size: [1, 1, 1],
                params: vec![],
                capabilities: vec![],
            }],
        },
    ])
}

#[test]
fn round_trips_targets_entries_and_parameters() {
    let bundle = parse(&sample()).unwrap();
    assert_eq!(bundle.targets.len(), 2);
    let metal = bundle.target_for(Backend::Metal).unwrap();
    assert_eq!(metal.code_format, "msl");
    assert_eq!(metal.layouts.len(), 2);
    let saxpy = metal.entry("saxpy").unwrap();
    assert_eq!(&*metal.layouts[saxpy.layout], b"saxpy layout");
    assert_eq!(
        &*metal.layouts[metal.entry("copy").unwrap().layout],
        b"copy layout"
    );
    assert_eq!(saxpy.capabilities, vec!["subgroup_basic".to_string()]);
    assert!(metal.entry("copy").unwrap().capabilities.is_empty());
    assert_eq!(saxpy.stage, Stage::Compute);
    assert_eq!(saxpy.thread_group_size, [64, 1, 1]);
    assert_eq!(saxpy.params[1], param("n", ArgKind::Value, 3));
    let wgsl = bundle.target_for(Backend::WebGpu).unwrap();
    assert!(wgsl.layouts.is_empty(), "wasm targets omit the layouts");
    assert_eq!(
        wgsl.entry("copy").unwrap().code,
        metal.entry("copy").unwrap().code
    );
    assert!(bundle.target_for(Backend::Vulkan).is_none());
}

#[test]
fn identical_blobs_are_stored_once() {
    let bytes = sample();
    let once = bytes
        .windows(b"shared code".len())
        .filter(|w| *w == b"shared code")
        .count();
    assert_eq!(once, 1);
}

#[test]
fn binding_layout_ids_match_the_parameter_kind() {
    let make = |p: Param| {
        write::bundle(&[TargetSpec {
            name: "wgsl",
            code_format: "wgsl",
            layouts: vec![],
            entries: vec![EntrySpec {
                name: "main",
                stage: "compute",
                layout: 0,
                code: b"shader",
                thread_group_size: [1, 1, 1],
                params: vec![p],
                capabilities: vec![],
            }],
        }])
    };
    for (index, layout) in dream_abi::gpu_bindings::BINDING_LAYOUTS.iter().enumerate() {
        let kind = ArgKind::from_i32(layout.kind as i32).unwrap();
        let mut p = param("resource", kind, 0);
        p.webgpu_layout = index as u32 + 1;
        let parsed = parse(&make(p.clone())).unwrap();
        assert_eq!(parsed.targets[0].entries[0].params[0], p);
        p.kind = if kind == ArgKind::Buffer {
            ArgKind::Sampler
        } else {
            ArgKind::Buffer
        };
        assert!(
            parse(&make(p.clone()))
                .unwrap_err()
                .to_string()
                .contains("binding layout")
        );
        p.webgpu_layout = u32::MAX;
        assert!(parse(&make(p)).is_err());
    }
}

#[test]
fn rejects_tampering_and_contract_violations() {
    let good = sample();
    let header_size = u32::from_le_bytes(good[8..12].try_into().unwrap()) as usize;

    let mut flipped = good.clone();
    *flipped.last_mut().unwrap() ^= 1;
    assert!(parse(&flipped).unwrap_err().to_string().contains("SHA-256"));

    let mut version = good.clone();
    version[4] = 1;
    assert!(parse(&version).is_err());

    let mut magic = good.clone();
    magic[0] = b'X';
    assert!(parse(&magic).is_err());

    let mut short_header = good.clone();
    short_header[8..12].copy_from_slice(&((header_size - 1) as u32).to_le_bytes());
    assert!(parse(&short_header).is_err());

    assert!(parse(&good[..header_size - 1]).is_err());

    let unknown_target = write::bundle(&[TargetSpec {
        name: "glsl",
        code_format: "glsl",
        layouts: vec![],
        entries: vec![],
    }]);
    assert!(parse(&unknown_target).is_err());

    let wrong_format = write::bundle(&[TargetSpec {
        name: "spirv",
        code_format: "wgsl",
        layouts: vec![],
        entries: vec![],
    }]);
    assert!(parse(&wrong_format).is_err());

    let unknown_stage = write::bundle(&[TargetSpec {
        name: "spirv",
        code_format: "spirv",
        layouts: vec![b"l"],
        entries: vec![EntrySpec {
            name: "main",
            stage: "geometry",
            layout: 0,
            code: b"c",
            thread_group_size: [0, 0, 0],
            params: vec![],
            capabilities: vec![],
        }],
    }]);
    assert!(parse(&unknown_stage).is_err());

    let missing_layout = write::bundle(&[TargetSpec {
        name: "spirv",
        code_format: "spirv",
        layouts: vec![b"l"],
        entries: vec![EntrySpec {
            name: "main",
            stage: "compute",
            layout: 1,
            code: b"c",
            thread_group_size: [1, 1, 1],
            params: vec![],
            capabilities: vec![],
        }],
    }]);
    assert!(
        parse(&missing_layout)
            .unwrap_err()
            .to_string()
            .contains("names layout")
    );
}

#[test]
fn rejects_unknown_kinds_and_flags() {
    let mut p = param("x", ArgKind::Buffer, 0);
    let make = |p: Param| {
        write::bundle(&[TargetSpec {
            name: "spirv",
            code_format: "spirv",
            layouts: vec![b"l"],
            entries: vec![EntrySpec {
                name: "main",
                stage: "compute",
                layout: 0,
                code: b"c",
                thread_group_size: [1, 1, 1],
                params: vec![p],
                capabilities: vec![],
            }],
        }])
    };
    assert!(parse(&make(p.clone())).is_ok());
    p.flags = 4;
    assert!(parse(&make(p.clone())).is_err());
    p.flags = 0;
    let mut bytes = make(p);
    // The kind follows the parameter's name; rewrite it to 9.
    let at = bytes.windows(5).position(|w| w == b"\x01\0\0\0x").unwrap() + 5;
    bytes[at..at + 4].copy_from_slice(&9u32.to_le_bytes());
    assert!(parse(&bytes).is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = parse(&bytes);
    }

    #[test]
    fn mutated_bundles_never_panic(
        edits in proptest::collection::vec((any::<usize>(), any::<u8>()), 1..8),
        cut in any::<usize>(),
    ) {
        let mut bytes = sample();
        for (at, value) in edits {
            let at = at % bytes.len();
            bytes[at] = value;
        }
        let keep = cut % (bytes.len() + 1);
        let _ = parse(&bytes[..keep]);
        let _ = parse(&bytes);
    }

    #[test]
    fn huge_counts_are_rejected_without_allocating(count in 1u32 << 20..u32::MAX) {
        let mut bytes = b"DGPB".to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        prop_assert!(parse(&bytes).is_err());
    }
}
