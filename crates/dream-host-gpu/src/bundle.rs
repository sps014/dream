//! Reader for DGPB v0 shader bundles, exactly as docs/internals/18-gpu-shader-bundle.md lays
//! them out. Bundles arrive from guest memory, so every count, offset and string is checked and
//! every blob's SHA-256 verified before anything uses it; malformed input is an error, never a
//! panic.

use crate::backend::Backend;
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Compute,
    Vertex,
    Fragment,
    RayGeneration,
    ClosestHit,
    AnyHit,
    Miss,
    Intersection,
    Callable,
}

impl Stage {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "compute" => Self::Compute,
            "vertex" => Self::Vertex,
            "fragment" => Self::Fragment,
            "raygeneration" => Self::RayGeneration,
            "closesthit" => Self::ClosestHit,
            "anyhit" => Self::AnyHit,
            "miss" => Self::Miss,
            "intersection" => Self::Intersection,
            "callable" => Self::Callable,
            _ => return None,
        })
    }
}

/// GpuArgKind numbering (crates/dream-stdlib/src/system/gpu/gpu_arg_kind.dream).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ArgKind {
    Buffer = 1,
    Texture = 2,
    Sampler = 3,
    StorageTexture = 4,
    AccelerationStructure = 5,
    Value = 6,
    MlpWeights = 7,
}

impl ArgKind {
    pub fn from_i32(value: i32) -> Option<Self> {
        Some(match value {
            1 => Self::Buffer,
            2 => Self::Texture,
            3 => Self::Sampler,
            4 => Self::StorageTexture,
            5 => Self::AccelerationStructure,
            6 => Self::Value,
            7 => Self::MlpWeights,
            _ => return None,
        })
    }
}

pub const FLAG_READ_ONLY: u32 = 1;
pub const FLAG_WRITTEN: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub kind: ArgKind,
    pub flags: u32,
    pub space: u32,
    pub binding: u32,
    pub offset: u32,
    pub size: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub stage: Stage,
    pub code: Arc<[u8]>,
    pub thread_group_size: [u32; 3],
    pub params: Vec<Param>,
    /// The capability names of docs/internals/18-gpu-shader-bundle.md this entry needs.
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub code_format: String,
    pub backend: Backend,
    pub layout: Arc<[u8]>,
    pub entries: Vec<Entry>,
}

impl Target {
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    pub targets: Vec<Target>,
}

impl Bundle {
    pub fn target_for(&self, backend: Backend) -> Option<&Target> {
        self.targets.iter().find(|t| t.backend == backend)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleError(String);

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "malformed shader bundle: {}", self.0)
    }
}

fn err<T>(message: impl Into<String>) -> Result<T, BundleError> {
    Err(BundleError(message.into()))
}

/// The slangc target names a v0 bundle may carry, with their code formats and backends.
fn target_backend(name: &str, format: &str) -> Result<Backend, BundleError> {
    let (backend, formats): (Backend, &[&str]) = match name {
        "dxil" => (Backend::Dx12, &["dxil"]),
        "dxbc" => (Backend::Dx11, &["dxbc"]),
        "spirv" => (Backend::Vulkan, &["spirv"]),
        "metal" => (Backend::Metal, &["metallib", "msl"]),
        "ptx" => (Backend::Cuda, &["ptx"]),
        "wgsl" => (Backend::WebGpu, &["wgsl"]),
        _ => return err(format!("unknown target `{name}`")),
    };
    if !formats.contains(&format) {
        return err(format!("target `{name}` cannot carry `{format}` code"));
    }
    Ok(backend)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> Result<u32, BundleError> {
        let Some(chunk) = self.bytes.get(self.pos..self.pos + 4) else {
            return err("truncated header");
        };
        self.pos += 4;
        Ok(u32::from_le_bytes(chunk.try_into().unwrap()))
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], BundleError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len());
        let Some(end) = end else {
            return err("truncated header");
        };
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn string(&mut self) -> Result<String, BundleError> {
        let len = self.u32()? as usize;
        let raw = self.take(len)?;
        match std::str::from_utf8(raw) {
            Ok(text) => Ok(text.to_string()),
            Err(_) => err("string is not UTF-8"),
        }
    }

    /// A count of items that each occupy at least `min_size` header bytes.
    fn count(&mut self, min_size: usize, what: &str) -> Result<usize, BundleError> {
        let n = self.u32()? as usize;
        let remaining = self.bytes.len() - self.pos;
        if n.saturating_mul(min_size) > remaining {
            return err(format!("{what} count {n} exceeds the header"));
        }
        Ok(n)
    }
}

struct Blobs<'a> {
    data: &'a [u8],
    verified: Vec<(u32, u32, Arc<[u8]>)>,
}

impl Blobs<'_> {
    fn read(&mut self, r: &mut Reader<'_>) -> Result<Arc<[u8]>, BundleError> {
        let offset = r.u32()?;
        let size = r.u32()?;
        let digest = r.take(32)?;
        if let Some((_, _, blob)) = self
            .verified
            .iter()
            .find(|(o, s, _)| *o == offset && *s == size)
        {
            if Sha256::digest(blob).as_slice() != digest {
                return err("blob SHA-256 mismatch");
            }
            return Ok(blob.clone());
        }
        let start = offset as usize;
        let Some(raw) = start
            .checked_add(size as usize)
            .and_then(|end| self.data.get(start..end))
        else {
            return err("blob lies outside the data section");
        };
        if Sha256::digest(raw).as_slice() != digest {
            return err("blob SHA-256 mismatch");
        }
        let blob: Arc<[u8]> = raw.into();
        self.verified.push((offset, size, blob.clone()));
        Ok(blob)
    }
}

// Smallest encodings, used to bound counts before allocating.
const MIN_PARAM: usize = 4 + 6 * 4;
const MIN_ENTRY: usize = 4 + 4 + 40 + 12 + 4 + 4;
const MIN_TARGET: usize = 4 + 4 + 40 + 4;

pub fn parse(bytes: &[u8]) -> Result<Bundle, BundleError> {
    let mut r = Reader { bytes, pos: 0 };
    if r.take(4).ok() != Some(b"DGPB".as_slice()) {
        return err("missing DGPB magic");
    }
    let version = r.u32()?;
    if version != 0 {
        return err(format!("unsupported version {version}"));
    }
    let header_size = r.u32()? as usize;
    if header_size < 16 || header_size > bytes.len() {
        return err("header size lies outside the bundle");
    }
    // Header fields never extend into the data section.
    let mut r = Reader {
        bytes: &bytes[..header_size],
        pos: 12,
    };
    let mut blobs = Blobs {
        data: &bytes[header_size..],
        verified: Vec::new(),
    };
    let target_count = r.count(MIN_TARGET, "target")?;
    let mut targets: Vec<Target> = Vec::with_capacity(target_count);
    for _ in 0..target_count {
        let name = r.string()?;
        let code_format = r.string()?;
        let backend = target_backend(&name, &code_format)?;
        if targets.iter().any(|t| t.name == name) {
            return err(format!("target `{name}` appears twice"));
        }
        let layout = blobs.read(&mut r)?;
        let entry_count = r.count(MIN_ENTRY, "entry")?;
        let mut entries: Vec<Entry> = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let entry_name = r.string()?;
            let stage_name = r.string()?;
            let Some(stage) = Stage::parse(&stage_name) else {
                return err(format!("unknown stage `{stage_name}`"));
            };
            if entries.iter().any(|e| e.name == entry_name) {
                return err(format!("entry `{entry_name}` appears twice in `{name}`"));
            }
            let code = blobs.read(&mut r)?;
            let thread_group_size = [r.u32()?, r.u32()?, r.u32()?];
            let param_count = r.count(MIN_PARAM, "parameter")?;
            let mut params = Vec::with_capacity(param_count);
            for _ in 0..param_count {
                let param_name = r.string()?;
                let kind_value = r.u32()?;
                let Some(kind) = ArgKind::from_i32(kind_value as i32) else {
                    return err(format!("unknown parameter kind {kind_value}"));
                };
                let flags = r.u32()?;
                if flags & !(FLAG_READ_ONLY | FLAG_WRITTEN) != 0 {
                    return err(format!("unknown parameter flags {flags:#x}"));
                }
                params.push(Param {
                    name: param_name,
                    kind,
                    flags,
                    space: r.u32()?,
                    binding: r.u32()?,
                    offset: r.u32()?,
                    size: r.u32()?,
                });
            }
            let capability_count = r.count(4, "capability")?;
            let mut capabilities = Vec::with_capacity(capability_count);
            for _ in 0..capability_count {
                capabilities.push(r.string()?);
            }
            entries.push(Entry {
                name: entry_name,
                stage,
                code,
                thread_group_size,
                params,
                capabilities,
            });
        }
        targets.push(Target {
            name,
            code_format,
            backend,
            layout,
            entries,
        });
    }
    if r.pos != header_size {
        return err("header size does not match the target table");
    }
    Ok(Bundle { targets })
}

/// Writes a v0 bundle; the inverse of [`parse`], for tests and test fixtures.
pub mod write {
    use super::*;

    pub struct EntrySpec<'a> {
        pub name: &'a str,
        pub stage: &'a str,
        pub code: &'a [u8],
        pub thread_group_size: [u32; 3],
        pub params: Vec<Param>,
        pub capabilities: Vec<&'a str>,
    }

    pub struct TargetSpec<'a> {
        pub name: &'a str,
        pub code_format: &'a str,
        pub layout: &'a [u8],
        pub entries: Vec<EntrySpec<'a>>,
    }

    pub fn bundle(targets: &[TargetSpec<'_>]) -> Vec<u8> {
        let mut data: Vec<u8> = Vec::new();
        let mut stored: Vec<(Vec<u8>, u32)> = Vec::new();
        let mut header: Vec<u8> = Vec::new();
        let u32s = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        let string = |out: &mut Vec<u8>, s: &str| {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        };
        let mut blob = |out: &mut Vec<u8>, raw: &[u8]| {
            let offset = match stored.iter().find(|(b, _)| b == raw) {
                Some((_, offset)) => *offset,
                None => {
                    let offset = data.len() as u32;
                    data.extend_from_slice(raw);
                    stored.push((raw.to_vec(), offset));
                    offset
                }
            };
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
            out.extend_from_slice(&Sha256::digest(raw));
        };
        u32s(&mut header, targets.len() as u32);
        for target in targets {
            string(&mut header, target.name);
            string(&mut header, target.code_format);
            blob(&mut header, target.layout);
            u32s(&mut header, target.entries.len() as u32);
            for entry in &target.entries {
                string(&mut header, entry.name);
                string(&mut header, entry.stage);
                blob(&mut header, entry.code);
                for axis in entry.thread_group_size {
                    u32s(&mut header, axis);
                }
                u32s(&mut header, entry.params.len() as u32);
                for p in &entry.params {
                    string(&mut header, &p.name);
                    for v in [
                        p.kind as u32,
                        p.flags,
                        p.space,
                        p.binding,
                        p.offset,
                        p.size,
                    ] {
                        u32s(&mut header, v);
                    }
                }
                u32s(&mut header, entry.capabilities.len() as u32);
                for capability in &entry.capabilities {
                    string(&mut header, capability);
                }
            }
        }
        let mut out = b"DGPB".to_vec();
        u32s(&mut out, 0);
        u32s(&mut out, (12 + header.len()) as u32);
        out.extend_from_slice(&header);
        out.extend_from_slice(&data);
        out
    }
}

#[cfg(test)]
#[path = "bundle_tests.rs"]
mod tests;
