//! Adapter capability negotiation: which optional features and raised limits Dream asks the device
//! for, and how the granted set is encoded for `Gpu.capabilities()`.
//!
//! Keep the packed layout below in sync with JS `gpuCapabilities` (`runtime/src/hosts/gpu.js`) and
//! the decoder in `crates/dream-stdlib/src/system/gpu/gpu_capabilities.dream`.

use super::state::lock_state;

/// Optional features Dream opts into whenever the adapter offers them.
///
/// WebGPU only lets a shader or resource touch a feature the *device* asked for; reaching for an
/// un-requested one is device-loss-grade rather than a recoverable validation error. That makes
/// this set a contract — it is exactly what `Gpu.capabilities()` reports as available, and nothing
/// outside it may be used even when the adapter physically supports it.
fn wanted_features() -> wgpu::Features {
    wgpu::Features::SHADER_F16
        | wgpu::Features::SUBGROUP
        | wgpu::Features::SUBGROUP_BARRIER
        | wgpu::Features::TEXTURE_COMPRESSION_BC
        | wgpu::Features::TEXTURE_COMPRESSION_ETC2
        | wgpu::Features::TEXTURE_COMPRESSION_ASTC
        | wgpu::Features::DEPTH32FLOAT_STENCIL8
        | wgpu::Features::FLOAT32_FILTERABLE
        | wgpu::Features::TIMESTAMP_QUERY
        | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
}

/// The subset of [`wanted_features`] this adapter can actually grant.
pub fn requested_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    adapter.features() & wanted_features()
}

/// Raises only the limits that large compute workloads run into, leaving everything else at the
/// portable WebGPU defaults so a program developed here still runs on a weaker device. Each field
/// saturates at the adapter's own maximum, so the request is supported by construction; where an
/// adapter reports *less* than the default the default is kept, matching prior behavior.
pub fn requested_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
    raise(wgpu::Limits::default(), adapter.limits())
}

fn raise(base: wgpu::Limits, max: wgpu::Limits) -> wgpu::Limits {
    wgpu::Limits {
        max_buffer_size: base.max_buffer_size.max(max.max_buffer_size),
        max_storage_buffer_binding_size: base
            .max_storage_buffer_binding_size
            .max(max.max_storage_buffer_binding_size),
        max_compute_workgroup_storage_size: base
            .max_compute_workgroup_storage_size
            .max(max.max_compute_workgroup_storage_size),
        max_compute_invocations_per_workgroup: base
            .max_compute_invocations_per_workgroup
            .max(max.max_compute_invocations_per_workgroup),
        max_compute_workgroup_size_x: base
            .max_compute_workgroup_size_x
            .max(max.max_compute_workgroup_size_x),
        max_compute_workgroup_size_y: base
            .max_compute_workgroup_size_y
            .max(max.max_compute_workgroup_size_y),
        max_compute_workgroup_size_z: base
            .max_compute_workgroup_size_z
            .max(max.max_compute_workgroup_size_z),
        max_compute_workgroups_per_dimension: base
            .max_compute_workgroups_per_dimension
            .max(max.max_compute_workgroups_per_dimension),
        ..base
    }
}

const FLAG_SHADER_FLOAT16: u32 = 1 << 0;
const FLAG_SUBGROUP: u32 = 1 << 1;
const FLAG_SUBGROUP_BARRIER: u32 = 1 << 2;
const FLAG_TILE_FLOAT16: u32 = 1 << 3;
const FLAG_TILE_FLOAT: u32 = 1 << 4;
const FLAG_TEXTURE_COMPRESSION_BC: u32 = 1 << 5;
const FLAG_TEXTURE_COMPRESSION_ETC2: u32 = 1 << 6;
const FLAG_TEXTURE_COMPRESSION_ASTC: u32 = 1 << 7;
const FLAG_DEPTH32_FLOAT_STENCIL8: u32 = 1 << 8;
const FLAG_FLOAT32_FILTERABLE: u32 = 1 << 9;
const FLAG_TIMESTAMP_QUERY: u32 = 1 << 10;
const FLAG_TIMESTAMP_QUERY_INSIDE_ENCODERS: u32 = 1 << 11;

/// Byte length of the packed blob. A decoder that sees fewer bytes reports nothing as available.
pub const BLOB_LEN: usize = 56;

/// Packs the device-granted features and limits little-endian. Returns an all-zero blob before
/// `try_init`, so callers see "nothing available" rather than a stale or optimistic answer.
pub fn encode() -> Vec<u8> {
    let st = lock_state();
    let Some(device) = st.device.as_ref() else {
        return vec![0u8; BLOB_LEN];
    };
    let features = device.features();
    let limits = device.limits();
    // Subgroup width is adapter information, not a requestable limit: the device echoes back the
    // zeroes `requested_limits` left in place, so read it from the adapter instead.
    let subgroup = st
        .adapter
        .as_ref()
        .map(|a| {
            let l = a.limits();
            (l.min_subgroup_size, l.max_subgroup_size)
        })
        .unwrap_or_default();

    let mut flags = 0u32;
    for (flag, feature) in [
        (FLAG_SHADER_FLOAT16, wgpu::Features::SHADER_F16),
        (FLAG_SUBGROUP, wgpu::Features::SUBGROUP),
        (FLAG_SUBGROUP_BARRIER, wgpu::Features::SUBGROUP_BARRIER),
        (
            FLAG_TEXTURE_COMPRESSION_BC,
            wgpu::Features::TEXTURE_COMPRESSION_BC,
        ),
        (
            FLAG_TEXTURE_COMPRESSION_ETC2,
            wgpu::Features::TEXTURE_COMPRESSION_ETC2,
        ),
        (
            FLAG_TEXTURE_COMPRESSION_ASTC,
            wgpu::Features::TEXTURE_COMPRESSION_ASTC,
        ),
        (
            FLAG_DEPTH32_FLOAT_STENCIL8,
            wgpu::Features::DEPTH32FLOAT_STENCIL8,
        ),
        (FLAG_FLOAT32_FILTERABLE, wgpu::Features::FLOAT32_FILTERABLE),
        (FLAG_TIMESTAMP_QUERY, wgpu::Features::TIMESTAMP_QUERY),
        (
            FLAG_TIMESTAMP_QUERY_INSIDE_ENCODERS,
            wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
        ),
    ] {
        if features.contains(feature) {
            flags |= flag;
        }
    }

    // Cooperative ("subgroup") matrix tiles need a `wgpu_cooperative_matrix`-capable naga, which
    // this pin predates. The bits and `tile_n` stay reserved so the layout does not shift once the
    // backend gains them; a caller gating on them today just takes the scalar path.
    flags &= !(FLAG_TILE_FLOAT16 | FLAG_TILE_FLOAT);
    let tile_n = 0u32;

    let mut out = Vec::with_capacity(BLOB_LEN);
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&tile_n.to_le_bytes());
    out.extend_from_slice(&limits.max_buffer_size.to_le_bytes());
    out.extend_from_slice(&u64::from(limits.max_storage_buffer_binding_size).to_le_bytes());
    out.extend_from_slice(&limits.max_compute_workgroup_storage_size.to_le_bytes());
    out.extend_from_slice(&limits.max_compute_invocations_per_workgroup.to_le_bytes());
    out.extend_from_slice(&limits.max_compute_workgroup_size_x.to_le_bytes());
    out.extend_from_slice(&limits.max_compute_workgroup_size_y.to_le_bytes());
    out.extend_from_slice(&limits.max_compute_workgroup_size_z.to_le_bytes());
    out.extend_from_slice(&limits.max_compute_workgroups_per_dimension.to_le_bytes());
    out.extend_from_slice(&subgroup.0.to_le_bytes());
    out.extend_from_slice(&subgroup.1.to_le_bytes());
    debug_assert_eq!(out.len(), BLOB_LEN);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn downlevel() -> wgpu::Limits {
        wgpu::Limits {
            max_buffer_size: 64 << 20,
            max_compute_invocations_per_workgroup: 64,
            ..wgpu::Limits::downlevel_defaults()
        }
    }

    #[test]
    fn raise_takes_the_adapter_maximum() {
        let base = wgpu::Limits::default();
        let max = wgpu::Limits {
            max_buffer_size: 8 << 30,
            max_storage_buffer_binding_size: u32::MAX,
            max_compute_workgroup_storage_size: 32768,
            max_compute_invocations_per_workgroup: 1024,
            max_compute_workgroup_size_x: 1024,
            max_compute_workgroup_size_y: 1024,
            max_compute_workgroup_size_z: 1024,
            max_compute_workgroups_per_dimension: 1 << 20,
            ..base.clone()
        };
        let got = raise(base.clone(), max.clone());
        assert_eq!(got.max_buffer_size, max.max_buffer_size);
        assert_eq!(
            got.max_storage_buffer_binding_size,
            max.max_storage_buffer_binding_size
        );
        assert_eq!(
            got.max_compute_workgroup_storage_size,
            max.max_compute_workgroup_storage_size
        );
        assert_eq!(
            got.max_compute_invocations_per_workgroup,
            max.max_compute_invocations_per_workgroup
        );
        assert_eq!(got.max_compute_workgroup_size_z, 1024);
        assert_eq!(got.max_compute_workgroups_per_dimension, 1 << 20);
        // Untouched fields stay at the portable default rather than following the adapter.
        assert_eq!(
            got.max_texture_dimension_2d,
            base.max_texture_dimension_2d
        );
    }

    /// An adapter that advertises *less* than the WebGPU default must not drag the request below
    /// it: that would silently shrink what already worked before features were negotiated.
    #[test]
    fn raise_never_drops_below_the_default() {
        let base = wgpu::Limits::default();
        let got = raise(base.clone(), downlevel());
        assert_eq!(got.max_buffer_size, base.max_buffer_size);
        assert_eq!(
            got.max_compute_invocations_per_workgroup,
            base.max_compute_invocations_per_workgroup
        );
    }

    /// Every bit Dream can report must correspond to a feature it actually asks the device for,
    /// or a program would gate on a capability that is never granted. The tile bits are the
    /// documented exception: reserved until a cooperative-matrix capable naga lands.
    #[test]
    fn reported_flags_are_all_requested() {
        let wanted = wanted_features();
        for (flag, feature) in [
            (FLAG_SHADER_FLOAT16, wgpu::Features::SHADER_F16),
            (FLAG_SUBGROUP, wgpu::Features::SUBGROUP),
            (FLAG_SUBGROUP_BARRIER, wgpu::Features::SUBGROUP_BARRIER),
            (
                FLAG_TEXTURE_COMPRESSION_BC,
                wgpu::Features::TEXTURE_COMPRESSION_BC,
            ),
            (
                FLAG_TEXTURE_COMPRESSION_ETC2,
                wgpu::Features::TEXTURE_COMPRESSION_ETC2,
            ),
            (
                FLAG_TEXTURE_COMPRESSION_ASTC,
                wgpu::Features::TEXTURE_COMPRESSION_ASTC,
            ),
            (
                FLAG_DEPTH32_FLOAT_STENCIL8,
                wgpu::Features::DEPTH32FLOAT_STENCIL8,
            ),
            (FLAG_FLOAT32_FILTERABLE, wgpu::Features::FLOAT32_FILTERABLE),
            (FLAG_TIMESTAMP_QUERY, wgpu::Features::TIMESTAMP_QUERY),
            (
                FLAG_TIMESTAMP_QUERY_INSIDE_ENCODERS,
                wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
            ),
        ] {
            assert!(
                wanted.contains(feature),
                "flag {:#x} is reported but never requested",
                flag
            );
        }
    }

    /// The Dream decoder reads fixed offsets out of this blob, so its length is ABI.
    #[test]
    fn encode_reports_nothing_before_init() {
        let blob = encode();
        assert_eq!(blob.len(), BLOB_LEN);
        assert!(blob.iter().all(|&b| b == 0));
    }
}
