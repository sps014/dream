//! Shader libraries (generated bundles) and the pipelines built from their entry points.

use crate::bundle::{self, Stage};
use crate::error::{Code, HostError, HostResult, check};
use crate::ffi;
use crate::state::{self, DeviceCore, Entry, Kind, Pipeline, PipelineEntry, PipelineKind, Program, State};
use std::ffi::CString;
use std::sync::Arc;

pub fn library_load(device: i32, bytes: &[u8]) -> HostResult<i32> {
    crate::strict_aot::check();
    let parsed = bundle::parse(bytes).map_err(|e| HostError::invalid(e.to_string()))?;
    let mut state = state::lock();
    let core = state.device(device)?;
    let Some(target) = parsed.target_for(core.backend) else {
        return Err(HostError::new(
            Code::NotCompiled,
            (core.backend as i32).to_string(),
        ));
    };
    if target.layouts.is_empty() || target.layouts.iter().any(|l| l.is_empty()) {
        return Err(HostError::invalid(format!(
            "the bundle's {} target has no layouts; native hosts need them",
            target.name
        )));
    }
    let target = Arc::new(target.clone());
    state.insert(Entry {
        kind: Kind::Library(target),
        device,
        last_use: 0,
        core,
    })
}

pub(crate) fn library(state: &State, handle: i32) -> HostResult<(Arc<bundle::Target>, i32, Arc<DeviceCore>)> {
    let entry = state.entry(handle)?;
    let Kind::Library(target) = &entry.kind else {
        return Err(HostError::invalid(format!(
            "handle {handle} is a {}, not a shader library",
            entry.kind.name()
        )));
    };
    entry.core.alive()?;
    Ok((target.clone(), entry.device, entry.core.clone()))
}

pub(crate) fn entry<'a>(target: &'a bundle::Target, name: &str, stage: Stage) -> HostResult<&'a bundle::Entry> {
    // Entry names come from wrappers generated with this bundle, so a missing one is a shader
    // whose `@gpu_targets` leave out this backend.
    let found = target
        .entry(name)
        .ok_or_else(|| HostError::new(Code::NotCompiled, (target.backend as i32).to_string()))?;
    if found.stage != stage {
        return Err(HostError::new(
            Code::Validation,
            format!("`{name}` is a {:?} shader, not {stage:?}", found.stage),
        ));
    }
    Ok(found)
}

/// `EUNSUPPORTED` with the first feature `entry` needs that `core` lacks; never a fallback.
pub(crate) fn require_capabilities(core: &DeviceCore, entry: &bundle::Entry) -> HostResult<()> {
    for name in &entry.capabilities {
        let bit = crate::error::feature::of_capability(name).ok_or_else(|| {
            HostError::invalid(format!("`{}` needs the unknown capability `{name}`", entry.name))
        })?;
        if !core.has_feature(bit) {
            return Err(HostError::unsupported(bit));
        }
    }
    Ok(())
}

/// One program per pipeline: the entries must come from one pipeline group, whose recorded
/// layout binds exactly their globals.
pub(crate) fn program(core: &DeviceCore, target: &bundle::Target, entries: &[&bundle::Entry]) -> HostResult<Program> {
    let group = entries.first().map_or(0, |e| e.layout);
    if let Some(other) = entries.iter().find(|e| e.layout != group) {
        return Err(HostError::invalid(format!(
            "`{}` and `{}` have different layouts, so they cannot share a pipeline",
            entries[0].name, other.name
        )));
    }
    let layout = target
        .layouts
        .get(group)
        .ok_or_else(|| HostError::invalid(format!("the bundle's {} target has no layout {group}", target.name)))?;
    let names: Vec<CString> = entries
        .iter()
        .map(|e| CString::new(e.name.as_str()).map_err(|_| HostError::invalid("entry point name contains NUL")))
        .collect::<HostResult<_>>()?;
    let code: Vec<ffi::EntryCode> = entries
        .iter()
        .zip(&names)
        .map(|(e, name)| ffi::EntryCode {
            name: name.as_ptr(),
            code: e.code.as_ptr().cast(),
            size: e.code.len(),
        })
        .collect();
    let mut raw = std::ptr::null_mut();
    check(unsafe {
        ffi::dgpu_program_create(
            core.raw(),
            layout.as_ptr().cast(),
            layout.len(),
            code.as_ptr(),
            code.len() as u32,
            &mut raw,
        )
    })?;
    Ok(Program::from_raw(raw))
}

/// The glue's view of each stage's parameter table; the names live in `names`.
pub(crate) struct StageTables {
    _names: Vec<Vec<CString>>,
    params: Vec<Vec<ffi::Param>>,
}

impl StageTables {
    pub(crate) fn new(stages: &[Vec<bundle::Param>]) -> HostResult<Self> {
        let mut names = Vec::new();
        let mut params = Vec::new();
        for stage in stages {
            let stage_names: Vec<CString> = stage
                .iter()
                .map(|p| CString::new(p.name.as_str()).map_err(|_| HostError::invalid("parameter name contains NUL")))
                .collect::<HostResult<_>>()?;
            params.push(
                stage
                    .iter()
                    .zip(&stage_names)
                    .map(|(p, name)| ffi::Param {
                        name: name.as_ptr(),
                        kind: p.kind as i32,
                        flags: p.flags,
                        space: p.space,
                        binding: p.binding,
                        offset: p.offset,
                        size: p.size,
                    })
                    .collect(),
            );
            names.push(stage_names);
        }
        Ok(Self {
            _names: names,
            params,
        })
    }

    pub(crate) fn views(&self) -> Vec<ffi::StageParams> {
        self.params
            .iter()
            .map(|p| ffi::StageParams {
                params: p.as_ptr(),
                count: p.len() as u32,
            })
            .collect()
    }
}

pub fn compute_pipeline_create(library_handle: i32, name: &str) -> HostResult<i32> {
    crate::strict_aot::check();
    let mut state = state::lock();
    let (target, device, core) = library(&state, library_handle)?;
    let compute = entry(&target, name, Stage::Compute)?;
    require_capabilities(&core, compute)?;
    let program = program(&core, &target, &[compute])?;
    let stages = vec![compute.params.clone()];
    let tables = StageTables::new(&stages)?;
    let views = tables.views();
    let mut raw = std::ptr::null_mut();
    check(unsafe { ffi::dgpu_compute_pipeline_create(core.raw(), program.raw(), views.as_ptr(), &mut raw) })?;
    state.insert(Entry {
        kind: Kind::Pipeline(PipelineEntry {
            pipeline: Pipeline::from_raw(raw),
            _program: program,
            kind: PipelineKind::Compute,
            stages,
        }),
        device,
        last_use: 0,
        core,
    })
}

/// Decodes GpuRenderPipelineDesc.encode.
fn render_desc(desc: &[i32]) -> HostResult<(ffi::RenderDesc, Vec<ffi::VertexBufferLayout>, Vec<ffi::VertexAttribute>)> {
    let malformed = || HostError::invalid("malformed render pipeline description");
    let head = desc.get(..9).ok_or_else(malformed)?;
    let in_range = |v: i32, max: i32| (0..=max).contains(&v);
    let valid = in_range(head[0], 8)
        && in_range(head[1], 4)
        && in_range(head[2], 2)
        && in_range(head[3], 3)
        && (head[4] == 0 || head[4] == 11 || head[4] == 12)
        && in_range(head[5], 7)
        && in_range(head[6], 1)
        && (head[7] == 1 || head[7] == 4);
    if !valid {
        return Err(malformed());
    }
    let mut buffers = Vec::new();
    let mut attributes = Vec::new();
    let mut at = 9;
    for slot in 0..head[8].max(0) as u32 {
        let layout = desc.get(at..at + 3).ok_or_else(malformed)?;
        if layout[0] < 0 || !in_range(layout[1], 1) || layout[2] < 0 {
            return Err(malformed());
        }
        buffers.push(ffi::VertexBufferLayout {
            stride: layout[0] as u32,
            per_instance: layout[1] as u32,
        });
        at += 3;
        for _ in 0..layout[2] {
            let a = desc.get(at..at + 3).ok_or_else(malformed)?;
            if a[0] < 0 || !in_range(a[1], 6) || a[2] < 0 {
                return Err(malformed());
            }
            attributes.push(ffi::VertexAttribute {
                location: a[0] as u32,
                format: a[1] as u32,
                offset: a[2] as u32,
                slot,
            });
            at += 3;
        }
    }
    if at != desc.len() || buffers.len() > 16 {
        return Err(malformed());
    }
    // Most backends take attribute i as shader location i, so locations must be 0..n-1.
    attributes.sort_by_key(|a| a.location);
    if attributes.iter().enumerate().any(|(i, a)| a.location != i as u32) {
        return Err(HostError::new(
            Code::Validation,
            "vertex attribute locations must be 0, 1, 2, … without gaps or repeats",
        ));
    }
    let render = ffi::RenderDesc {
        color_format: head[0] as u32,
        topology: head[1] as u32,
        cull: head[2] as u32,
        blend: head[3] as u32,
        depth_format: head[4] as u32,
        depth_compare: head[5] as u32,
        depth_write: head[6] as u32,
        samples: head[7] as u32,
        buffers: std::ptr::null(),
        buffer_count: 0,
        attributes: std::ptr::null(),
        attribute_count: 0,
    };
    Ok((render, buffers, attributes))
}

pub fn render_pipeline_create(
    vertex_library: i32,
    vertex_name: &str,
    fragment_library: i32,
    fragment_name: &str,
    desc: &[i32],
) -> HostResult<i32> {
    crate::strict_aot::check();
    let (mut render, buffers, attributes) = render_desc(desc)?;
    let mut state = state::lock();
    let (target, device, core) = library(&state, vertex_library)?;
    let (_, fragment_device, _) = library(&state, fragment_library)?;
    if fragment_device != device {
        return Err(HostError::new(Code::WrongDevice, "fragment shader library"));
    }
    if fragment_library != vertex_library {
        return Err(HostError::new(
            Code::Validation,
            "the vertex and fragment shaders of a pipeline must come from the same shader module",
        ));
    }
    let vertex = entry(&target, vertex_name, Stage::Vertex)?;
    let fragment = entry(&target, fragment_name, Stage::Fragment)?;
    require_capabilities(&core, vertex)?;
    require_capabilities(&core, fragment)?;
    let program = program(&core, &target, &[vertex, fragment])?;
    let stages = vec![vertex.params.clone(), fragment.params.clone()];
    let tables = StageTables::new(&stages)?;
    let views = tables.views();
    render.buffers = buffers.as_ptr();
    render.buffer_count = buffers.len() as u32;
    render.attributes = attributes.as_ptr();
    render.attribute_count = attributes.len() as u32;
    let mut raw = std::ptr::null_mut();
    check(unsafe {
        ffi::dgpu_render_pipeline_create(core.raw(), program.raw(), views.as_ptr(), &render, &mut raw)
    })?;
    state.insert(Entry {
        kind: Kind::Pipeline(PipelineEntry {
            pipeline: Pipeline::from_raw(raw),
            _program: program,
            kind: PipelineKind::Render,
            stages,
        }),
        device,
        last_use: 0,
        core,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_point_color_targets_preserve_the_requested_format() {
        for format in [5, 6, 7, 8] {
            let (render, _, _) = render_desc(&[format, 0, 0, 0, 0, 1, 1, 1, 0]).unwrap();
            assert_eq!(render.color_format, format as u32);
        }
        for format in [9, 10, 11] {
            assert!(render_desc(&[format, 0, 0, 0, 0, 1, 1, 1, 0]).is_err());
        }
    }

    #[test]
    fn render_descriptions_decode_vertex_layouts() {
        let desc = [2, 0, 2, 1, 11, 1, 1, 1, 2, 20, 0, 2, 1, 2, 0, 0, 1, 12, 16, 1, 1, 2, 3, 0];
        let (render, buffers, attributes) = render_desc(&desc).unwrap();
        assert_eq!((render.color_format, render.depth_format, render.samples), (2, 11, 1));
        assert_eq!(buffers.len(), 2);
        assert_eq!((buffers[1].stride, buffers[1].per_instance), (16, 1));
        assert_eq!(attributes.len(), 3);
        assert_eq!(
            (attributes[0].location, attributes[0].format, attributes[0].offset),
            (0, 1, 12),
            "attributes are sorted by location"
        );
        assert_eq!(
            (attributes[2].location, attributes[2].format, attributes[2].slot),
            (2, 3, 1)
        );
        let gap = [2, 0, 2, 1, 0, 1, 1, 1, 1, 8, 0, 1, 1, 1, 0];
        assert_eq!(render_desc(&gap).err().unwrap().code, Code::Validation);
    }

    #[test]
    fn malformed_render_descriptions_are_rejected() {
        for desc in [
            &[][..],
            &[2, 0, 2, 1, 0, 1, 1, 1][..],
            &[9, 0, 2, 1, 0, 1, 1, 1, 0][..],
            &[2, 0, 2, 1, 0, 1, 1, 3, 0][..],
            &[2, 0, 2, 1, 0, 1, 1, 1, 1, 16, 0, 1][..],
            &[2, 0, 2, 1, 0, 1, 1, 1, 0, 99][..],
            &[2, 0, 2, 1, 0, 1, 1, 1, 1, 16, 0, -1][..],
        ] {
            assert!(render_desc(desc).is_err(), "{desc:?}");
        }
    }
}
