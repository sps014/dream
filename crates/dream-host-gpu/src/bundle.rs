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
    pub webgpu_layout: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub stage: Stage,
    /// Indexes [`Target::layouts`]: the recorded layout of this entry's pipeline group.
    pub layout: usize,
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
    /// One recorded layout per pipeline group; none in wasm bundles.
    pub layouts: Vec<Arc<[u8]>>,
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

unsafe extern "C" {
    fn dream_gpu_bundle_abi_version() -> u32;
    fn dream_gpu_bundle_index(
        bytes: *const u8,
        length: u32,
        records: *mut u32,
        capacity: u32,
    ) -> i32;
    fn dream_gpu_bundle_error(status: i32) -> *const std::ffi::c_char;
}

fn status(value: i32) -> Result<usize, BundleError> {
    if value < 0 {
        let message = unsafe { std::ffi::CStr::from_ptr(dream_gpu_bundle_error(value)) };
        return err(message.to_string_lossy());
    }
    Ok(value as usize)
}

fn text(bytes: &[u8], offset: u32, size: u32) -> Result<String, BundleError> {
    let start = offset as usize;
    let raw = bytes
        .get(start..start.saturating_add(size as usize))
        .ok_or_else(|| BundleError("string lies outside the bundle".into()))?;
    std::str::from_utf8(raw)
        .map(str::to_owned)
        .map_err(|_| BundleError("string is not UTF-8".into()))
}

fn blob(
    bytes: &[u8],
    row: &[u32],
    cache: &mut std::collections::BTreeMap<(u32, u32), Arc<[u8]>>,
) -> Result<Arc<[u8]>, BundleError> {
    let start = row[0] as usize;
    let raw = bytes
        .get(start..start.saturating_add(row[1] as usize))
        .ok_or_else(|| BundleError("blob lies outside the bundle".into()))?;
    let digest = bytes
        .get(row[2] as usize..row[2] as usize + 32)
        .ok_or_else(|| BundleError("blob has no digest".into()))?;
    if Sha256::digest(raw).as_slice() != digest {
        return err("blob SHA-256 mismatch");
    }
    Ok(cache
        .entry((row[0], row[1]))
        .or_insert_with(|| Arc::from(raw))
        .clone())
}

pub fn parse(bytes: &[u8]) -> Result<Bundle, BundleError> {
    if unsafe { dream_gpu_bundle_abi_version() } != dream_abi::gpu_bindings::GPU_BUNDLE_ABI_VERSION
    {
        return err("GPU bundle capability ABI version mismatch");
    }
    let length =
        u32::try_from(bytes.len()).map_err(|_| BundleError("bundle is too large".into()))?;
    // The C reader accepts borrowed spans and writes only complete fixed-width records.
    // Neither pass retains its input or calls a host allocator.
    let count =
        status(unsafe { dream_gpu_bundle_index(bytes.as_ptr(), length, std::ptr::null_mut(), 0) })?;
    let mut records = vec![[0u32; 16]; count];
    status(unsafe {
        dream_gpu_bundle_index(
            bytes.as_ptr(),
            length,
            records.as_mut_ptr().cast(),
            count as u32,
        )
    })?;
    let mut targets: Vec<Target> = Vec::new();
    let mut parents: Vec<(usize, Option<usize>)> = vec![(0, None); count];
    let mut blobs = std::collections::BTreeMap::new();
    let stages = [
        Stage::Compute,
        Stage::Vertex,
        Stage::Fragment,
        Stage::RayGeneration,
        Stage::ClosestHit,
        Stage::AnyHit,
        Stage::Miss,
        Stage::Intersection,
        Stage::Callable,
    ];
    for (index, row) in records.iter().enumerate() {
        if row[0] == 1 {
            let backend = Backend::from_i32(row[6] as i32)
                .ok_or_else(|| BundleError("unknown target".into()))?;
            parents[index] = (targets.len(), None);
            targets.push(Target {
                name: text(bytes, row[2], row[3])?,
                code_format: text(bytes, row[4], row[5])?,
                backend,
                layouts: Vec::new(),
                entries: Vec::new(),
            });
            continue;
        }
        let &(target_index, entry_index) = parents
            .get(row[1] as usize)
            .ok_or_else(|| BundleError("invalid record parent".into()))?;
        let target = targets
            .get_mut(target_index)
            .ok_or_else(|| BundleError("missing target record".into()))?;
        match row[0] {
            2 => target.layouts.push(blob(bytes, &row[2..5], &mut blobs)?),
            3 => {
                parents[index] = (target_index, Some(target.entries.len()));
                target.entries.push(Entry {
                    name: text(bytes, row[2], row[3])?,
                    stage: *stages
                        .get(row[4] as usize)
                        .ok_or_else(|| BundleError("unknown stage".into()))?,
                    layout: row[5] as usize,
                    code: blob(bytes, &row[6..9], &mut blobs)?,
                    thread_group_size: [row[9], row[10], row[11]],
                    params: Vec::new(),
                    capabilities: Vec::new(),
                });
            }
            4 | 5 => {
                let entry = entry_index
                    .and_then(|i| target.entries.get_mut(i))
                    .ok_or_else(|| BundleError("missing entry record".into()))?;
                let name = text(bytes, row[2], row[3])?;
                if row[0] == 5 {
                    entry.capabilities.push(name);
                } else {
                    entry.params.push(Param {
                        name,
                        kind: ArgKind::from_i32(row[4] as i32)
                            .ok_or_else(|| BundleError("unknown parameter kind".into()))?,
                        flags: row[5],
                        space: row[6],
                        binding: row[7],
                        offset: row[8],
                        size: row[9],
                        webgpu_layout: row[10],
                    });
                }
            }
            _ => return err("invalid bundle record"),
        }
    }
    Ok(Bundle { targets })
}

/// Writes a v0 bundle; the inverse of [`parse`], for tests and test fixtures.
#[path = "bundle_write.rs"]
pub mod write;

#[cfg(test)]
#[path = "bundle_tests.rs"]
mod tests;
