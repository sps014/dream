//! Ray tracing: acceleration structure builds and ray tracing pipelines. The guest's encodings
//! (GpuGeometry.encode, GpuInstance.encode, the GpuDevice.ray_tracing_pipeline layout) are
//! decoded and validated here, before anything reaches the glue.

use crate::bundle::{self, Stage};
use crate::error::{self, Code, HostError, HostResult, check, feature};
use crate::ffi;
use crate::pipelines::{StageTables, entry, library, program, require_capabilities};
use crate::resources::encode;
use crate::state::{
    self, AccelerationStructureEntry, DeviceCore, Entry, Kind, Obj, Pipeline, PipelineEntry, PipelineKind,
    State,
};
use std::ffi::CString;
use std::sync::Arc;

/// Bytes of one `GpuVec3`: positions of triangles, and each corner of a box.
const VEC3_BYTES: u32 = 12;
/// The ray payload and hit attribute sizes every pipeline reserves. The generator does not
/// record payload sizes yet; DXR rejects payloads larger than the pipeline declares.
const MAX_PAYLOAD_BYTES: u32 = 128;
const MAX_ATTRIBUTE_BYTES: u32 = 32;
/// Rays traced from ray generation shaders only; hit shaders do not trace further rays.
const MAX_RECURSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryKind {
    Triangles,
    Boxes,
}

/// A decoded `GpuGeometry.encode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub kind: GeometryKind,
    pub vertices: i32,
    pub vertex_count: u32,
    pub indices: Option<(i32, u32)>,
    pub opaque: bool,
}

/// `[kind, vertex_buffer, vertex_count, index_buffer or 0, index_count, opaque]`.
pub fn decode_geometry(words: &[i32]) -> HostResult<Geometry> {
    let &[kind, vertices, vertex_count, indices, index_count, opaque] = words else {
        return Err(HostError::invalid("a geometry description has 6 values"));
    };
    let kind = match kind {
        0 => GeometryKind::Triangles,
        1 => GeometryKind::Boxes,
        _ => return Err(HostError::invalid(format!("unknown geometry kind {kind}"))),
    };
    let count = |value: i32, what: &str| {
        u32::try_from(value).map_err(|_| HostError::invalid(format!("{what} must not be negative")))
    };
    let vertex_count = count(vertex_count, "the vertex count")?;
    let index_count = count(index_count, "the index count")?;
    if vertex_count == 0 {
        return Err(HostError::invalid("geometry needs at least one primitive"));
    }
    if !(0..=1).contains(&opaque) {
        return Err(HostError::invalid("geometry opacity is 0 or 1"));
    }
    let indices = (indices != 0).then_some((indices, index_count));
    match (kind, indices) {
        (GeometryKind::Boxes, Some(_)) => return Err(HostError::invalid("boxes take no index buffer")),
        (GeometryKind::Boxes, None) if !vertex_count.is_multiple_of(2) => {
            return Err(HostError::invalid("each box is a min and a max corner, so the count is even"));
        }
        (GeometryKind::Triangles, Some((_, n))) if n == 0 || !n.is_multiple_of(3) => {
            return Err(HostError::invalid("triangle indices come in threes"));
        }
        (GeometryKind::Triangles, None) if !vertex_count.is_multiple_of(3) => {
            return Err(HostError::invalid("unindexed triangle vertices come in threes"));
        }
        _ => {}
    }
    Ok(Geometry {
        kind,
        vertices,
        vertex_count,
        indices,
        opaque: opaque == 1,
    })
}

/// A decoded `GpuInstance.encode` record with its transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstanceRecord {
    pub bottom_level: i32,
    pub id: u32,
    pub mask: u32,
    pub hit_group_offset: u32,
    pub flags: u32,
    /// Row-major 3x4 object-to-world matrix.
    pub transform: [f32; 12],
}

const RECORD_WORDS: usize = 5;
const TRANSFORM_FLOATS: usize = 12;

/// `records` holds `[bottom_level, id, mask, hit_group_offset, flags]` per instance and
/// `transforms` the matching row-major 3x4 matrices.
pub fn decode_instances(records: &[i32], transforms: &[f32]) -> HostResult<Vec<InstanceRecord>> {
    if !records.len().is_multiple_of(RECORD_WORDS) || transforms.len() != records.len() / RECORD_WORDS * TRANSFORM_FLOATS {
        return Err(HostError::invalid("instance records and transforms do not match"));
    }
    if records.is_empty() {
        return Err(HostError::invalid("a top-level structure needs at least one instance"));
    }
    records
        .chunks(RECORD_WORDS)
        .zip(transforms.chunks(TRANSFORM_FLOATS))
        .map(|(r, t)| {
            let field = |value: i32, bits: u32, what: &str| {
                u32::try_from(value)
                    .ok()
                    .filter(|v| *v < (1 << bits))
                    .ok_or_else(|| HostError::invalid(format!("instance {what} {value} does not fit in {bits} bits")))
            };
            if t.iter().any(|v| !v.is_finite()) {
                return Err(HostError::invalid("instance transforms must be finite"));
            }
            Ok(InstanceRecord {
                bottom_level: r[0],
                id: field(r[1], 24, "id")?,
                mask: field(r[2], 8, "mask")?,
                hit_group_offset: field(r[3], 24, "hit group offset")?,
                flags: field(r[4], 8, "flags")?,
                transform: t.try_into().unwrap(),
            })
        })
        .collect()
}

/// One hit group: entry indexes of its closest hit, any hit and intersection shaders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HitGroup {
    pub closest_hit: usize,
    pub any_hit: Option<usize>,
    pub intersection: Option<usize>,
}

/// A decoded GpuDevice.ray_tracing_pipeline layout; every value indexes the entry list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub ray_generation: usize,
    pub misses: Vec<usize>,
    pub hit_groups: Vec<HitGroup>,
    pub callables: Vec<usize>,
}

impl Layout {
    /// `[ray_generation, miss_count, misses..., group_count, (closest_hit, any_hit,
    /// intersection)..., callable_count, callables...]`; absent hit group members are -1.
    pub fn decode(words: &[i32], entry_count: usize) -> HostResult<Self> {
        let malformed = || HostError::invalid("malformed ray tracing pipeline layout");
        let mut at = 0;
        let mut next = || {
            let word = *words.get(at).ok_or_else(malformed)?;
            at += 1;
            Ok::<i32, HostError>(word)
        };
        let index = |word: i32| {
            usize::try_from(word)
                .ok()
                .filter(|i| *i < entry_count)
                .ok_or_else(|| HostError::invalid(format!("ray tracing layout names entry {word} of {entry_count}")))
        };
        let optional = |word: i32| if word == -1 { Ok(None) } else { index(word).map(Some) };
        let ray_generation = index(next()?)?;
        let list = |next: &mut dyn FnMut() -> HostResult<i32>| -> HostResult<Vec<usize>> {
            let count = usize::try_from(next()?).map_err(|_| malformed())?;
            if count > entry_count {
                return Err(malformed());
            }
            (0..count).map(|_| index(next()?)).collect()
        };
        let misses = list(&mut next)?;
        let group_count = usize::try_from(next()?).map_err(|_| malformed())?;
        if group_count > entry_count {
            return Err(malformed());
        }
        let mut hit_groups = Vec::with_capacity(group_count);
        for _ in 0..group_count {
            hit_groups.push(HitGroup {
                closest_hit: index(next()?)?,
                any_hit: optional(next()?)?,
                intersection: optional(next()?)?,
            });
        }
        let callables = list(&mut next)?;
        if at != words.len() {
            return Err(malformed());
        }
        Ok(Self {
            ray_generation,
            misses,
            hit_groups,
            callables,
        })
    }

    /// Each entry index with the stage it plays, in program order: ray generation first,
    /// then misses, hit group members and callables, each index once.
    pub fn program_entries(&self) -> HostResult<Vec<(usize, Stage)>> {
        let mut out: Vec<(usize, Stage)> = Vec::new();
        let mut add = |index: usize, stage: Stage| -> HostResult<()> {
            match out.iter().find(|(i, _)| *i == index) {
                Some((_, seen)) if *seen != stage => Err(HostError::new(
                    Code::Validation,
                    format!("ray tracing entry {index} is used as both {seen:?} and {stage:?}"),
                )),
                Some(_) => Ok(()),
                None => {
                    out.push((index, stage));
                    Ok(())
                }
            }
        };
        add(self.ray_generation, Stage::RayGeneration)?;
        for &miss in &self.misses {
            add(miss, Stage::Miss)?;
        }
        for group in &self.hit_groups {
            add(group.closest_hit, Stage::ClosestHit)?;
            if let Some(any_hit) = group.any_hit {
                add(any_hit, Stage::AnyHit)?;
            }
            if let Some(intersection) = group.intersection {
                add(intersection, Stage::Intersection)?;
            }
        }
        for &callable in &self.callables {
            add(callable, Stage::Callable)?;
        }
        Ok(out)
    }
}

fn require_acceleration_structures(core: &DeviceCore) -> HostResult<()> {
    if core.has_feature(feature::RAY_TRACING) || core.has_feature(feature::RAY_QUERY) {
        Ok(())
    } else {
        Err(HostError::unsupported(feature::RAY_TRACING))
    }
}

/// The raw buffer behind `handle`, checked to belong to `device` and to hold `bytes`.
fn build_buffer(state: &State, handle: i32, device: i32, bytes: u64, what: &str) -> HostResult<*mut ffi::DgpuObject> {
    let entry = state.owned(handle, device, what)?;
    let Kind::Buffer(buffer) = &entry.kind else {
        return Err(HostError::invalid(format!("the {what} is a {}", entry.kind.name())));
    };
    if bytes > buffer.size {
        return Err(HostError::invalid(format!(
            "the {what} holds {} bytes, the geometry needs {bytes}",
            buffer.size
        )));
    }
    Ok(buffer.obj.raw())
}

/// Inserts the structure now and resolves `future` with its handle once the build finished
/// (0 and the build's error if it failed). `transient` lives until then.
fn finish_build(
    state: &mut State,
    device: i32,
    core: Arc<DeviceCore>,
    future: usize,
    fence: u64,
    built: AccelerationStructureEntry,
    transient: Vec<Obj>,
) -> HostResult<()> {
    let handle = state.insert(Entry {
        kind: Kind::AccelerationStructure(built),
        device,
        last_use: fence,
        core: core.clone(),
    })?;
    core.push(
        fence,
        future,
        Box::new(move |state, waited| {
            drop(transient);
            match waited {
                Ok(()) => handle as u64,
                Err(e) => {
                    error::record(&e);
                    state.release(handle);
                    0
                }
            }
        }),
    );
    Ok(())
}

pub fn bottom_level_build(future: usize, device: i32, words: &[i32]) {
    if let Err(e) = bottom_level_build_now(future, device, words) {
        error::record(&e);
        crate::waiter::complete(future, 0);
    }
}

fn bottom_level_build_now(future: usize, device: i32, words: &[i32]) -> HostResult<()> {
    let geometry = decode_geometry(words)?;
    let mut state = state::lock();
    let core = state.device(device)?;
    require_acceleration_structures(&core)?;
    let vertex_bytes = u64::from(geometry.vertex_count) * u64::from(VEC3_BYTES);
    let vertices = build_buffer(&state, geometry.vertices, device, vertex_bytes, "geometry buffer")?;
    let (indices, index_count) = match geometry.indices {
        Some((handle, count)) => (
            build_buffer(&state, handle, device, u64::from(count) * 4, "index buffer")?,
            count,
        ),
        None => (std::ptr::null_mut(), 0),
    };
    let desc = ffi::Geometry {
        kind: (geometry.kind == GeometryKind::Boxes) as i32,
        vertices,
        vertex_count: geometry.vertex_count,
        vertex_stride: VEC3_BYTES,
        indices,
        index_count,
        opaque: geometry.opaque as i32,
    };
    let mut out = std::ptr::null_mut();
    let mut scratch = std::ptr::null_mut();
    let fence = encode(&core, |e| {
        check(unsafe { ffi::dgpu_enc_build_bottom_level(e, &desc, &mut out, &mut scratch) })
    })?;
    state.touch(geometry.vertices, fence);
    if let Some((handle, _)) = geometry.indices {
        state.touch(handle, fence);
    }
    let built = AccelerationStructureEntry {
        obj: Obj::from_raw(out),
        top_level: false,
        bottom_levels: Vec::new(),
    };
    finish_build(&mut state, device, core, future, fence, built, vec![Obj::from_raw(scratch)])
}

pub fn top_level_build(future: usize, device: i32, records: &[i32], transforms: &[f32]) {
    if let Err(e) = top_level_build_now(future, device, records, transforms) {
        error::record(&e);
        crate::waiter::complete(future, 0);
    }
}

fn top_level_build_now(future: usize, device: i32, records: &[i32], transforms: &[f32]) -> HostResult<()> {
    let instances = decode_instances(records, transforms)?;
    let mut state = state::lock();
    let core = state.device(device)?;
    require_acceleration_structures(&core)?;
    let mut bottom_levels = Vec::with_capacity(instances.len());
    let mut descs = Vec::with_capacity(instances.len());
    for instance in &instances {
        let entry = state.owned(instance.bottom_level, device, "instance acceleration structure")?;
        let Kind::AccelerationStructure(blas) = &entry.kind else {
            return Err(HostError::invalid(format!(
                "an instance names a {}, not an acceleration structure",
                entry.kind.name()
            )));
        };
        if blas.top_level {
            return Err(HostError::invalid("instances need bottom-level acceleration structures"));
        }
        descs.push(ffi::Instance {
            bottom_level: blas.obj.raw(),
            transform: instance.transform,
            id: instance.id,
            mask: instance.mask,
            hit_group_offset: instance.hit_group_offset,
            flags: instance.flags,
        });
        bottom_levels.push(blas.obj.share());
    }
    let mut out = std::ptr::null_mut();
    let mut scratch = std::ptr::null_mut();
    let mut instance_buffer = std::ptr::null_mut();
    let fence = encode(&core, |e| {
        check(unsafe {
            ffi::dgpu_enc_build_top_level(
                e,
                descs.as_ptr(),
                descs.len() as u32,
                &mut out,
                &mut scratch,
                &mut instance_buffer,
            )
        })
    })?;
    for instance in &instances {
        state.touch(instance.bottom_level, fence);
    }
    let built = AccelerationStructureEntry {
        obj: Obj::from_raw(out),
        top_level: true,
        bottom_levels,
    };
    let transient = vec![Obj::from_raw(scratch), Obj::from_raw(instance_buffer)];
    finish_build(&mut state, device, core, future, fence, built, transient)
}

/// `entries` are newline-separated names, `libraries[i]` the library of entry `i`.
pub fn ray_tracing_pipeline_create(device: i32, libraries: &[i32], entries: &str, layout: &[i32]) -> HostResult<i32> {
    crate::strict_aot::check();
    let names: Vec<&str> = if entries.is_empty() { Vec::new() } else { entries.split('\n').collect() };
    if names.len() != libraries.len() || names.is_empty() {
        return Err(HostError::invalid("every ray tracing entry needs its library"));
    }
    let layout = Layout::decode(layout, names.len())?;
    let roles = layout.program_entries()?;
    let mut state = state::lock();
    let core = state.device(device)?;
    if !core.has_feature(feature::RAY_TRACING) {
        return Err(HostError::unsupported(feature::RAY_TRACING));
    }
    if libraries.iter().any(|l| *l != libraries[0]) {
        return Err(HostError::new(
            Code::Validation,
            "the shaders of one ray tracing pipeline must come from one shader module",
        ));
    }
    let (target, library_device, _) = library(&state, libraries[0])?;
    if library_device != device {
        return Err(HostError::new(Code::WrongDevice, "ray tracing shader library"));
    }
    let resolved: Vec<&bundle::Entry> = roles
        .iter()
        .map(|&(index, stage)| entry(&target, names[index], stage))
        .collect::<HostResult<_>>()?;
    for e in &resolved {
        require_capabilities(&core, e)?;
    }
    let program = program(&core, &target, &resolved)?;
    let stages: Vec<Vec<bundle::Param>> = resolved.iter().map(|e| e.params.clone()).collect();
    let tables = StageTables::new(&stages)?;
    let views = tables.views();

    let c_names: Vec<CString> = names
        .iter()
        .map(|n| CString::new(*n).map_err(|_| HostError::invalid("entry point name contains NUL")))
        .collect::<HostResult<_>>()?;
    let name = |index: usize| c_names[index].as_ptr();
    let optional = |index: Option<usize>| index.map_or(std::ptr::null(), name);
    let misses: Vec<_> = layout.misses.iter().map(|&i| name(i)).collect();
    let callables: Vec<_> = layout.callables.iter().map(|&i| name(i)).collect();
    let groups: Vec<ffi::HitGroup> = layout
        .hit_groups
        .iter()
        .map(|g| ffi::HitGroup {
            closest_hit: name(g.closest_hit),
            any_hit: optional(g.any_hit),
            intersection: optional(g.intersection),
        })
        .collect();
    let desc = ffi::RayTracingDesc {
        ray_generation: name(layout.ray_generation),
        misses: misses.as_ptr(),
        miss_count: misses.len() as u32,
        hit_groups: groups.as_ptr(),
        hit_group_count: groups.len() as u32,
        callables: callables.as_ptr(),
        callable_count: callables.len() as u32,
        max_payload_size: MAX_PAYLOAD_BYTES,
        max_attribute_size: MAX_ATTRIBUTE_BYTES,
        max_recursion: MAX_RECURSION,
    };
    let mut raw = std::ptr::null_mut();
    check(unsafe {
        ffi::dgpu_ray_tracing_pipeline_create(
            core.raw(),
            program.raw(),
            views.as_ptr(),
            views.len() as u32,
            &desc,
            &mut raw,
        )
    })?;
    state.insert(Entry {
        kind: Kind::Pipeline(PipelineEntry {
            pipeline: Pipeline::from_raw(raw),
            _program: program,
            kind: PipelineKind::RayTracing,
            stages,
        }),
        device,
        last_use: 0,
        core,
    })
}

#[cfg(test)]
#[path = "raytracing_tests.rs"]
mod tests;
