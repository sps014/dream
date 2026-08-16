//! Parsed `"gpu"` section from sibling `.abi.json`.

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GpuAbi {
    #[serde(default)]
    pub kernels: Vec<GpuKernelMeta>,
    #[serde(default)]
    pub shaders: Vec<GpuShaderMeta>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpuKernelMeta {
    pub name: String,
    pub entry: String,
    #[serde(default = "default_workgroup")]
    pub workgroup: [u32; 3],
    #[serde(default)]
    pub bindings: Vec<GpuBindingMeta>,
    #[serde(default)]
    pub source: String,
}

fn default_workgroup() -> [u32; 3] {
    [64, 1, 1]
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpuShaderMeta {
    pub name: String,
    pub stage: String,
    pub entry: String,
    #[serde(default)]
    pub bindings: Vec<GpuBindingMeta>,
    #[serde(default)]
    pub vertex_layout: Vec<GpuVertexAttrMeta>,
    #[serde(default)]
    pub vertex_stride: u32,
    #[serde(default = "default_color_targets")]
    pub color_targets: u32,
    #[serde(default)]
    pub source: String,
}

fn default_color_targets() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpuBindingMeta {
    pub binding: u32,
    pub kind: String,
    #[serde(default)]
    pub read_write: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GpuVertexAttrMeta {
    pub location: u32,
    pub format: String,
    pub offset: u32,
}

#[derive(Debug, Deserialize)]
struct AbiFile {
    #[serde(default)]
    gpu: Option<GpuAbi>,
}

/// `.abi.json` next to an artifact, or the path itself if it already is that file.
pub fn abi_json_path(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.ends_with(".abi.json") {
        path.to_path_buf()
    } else {
        path.with_extension("abi.json")
    }
}

/// Load `abi.gpu` from the sibling `.abi.json` next to a `.wat` / `.wasm` / `.out` path.
pub fn load_gpu_abi_beside(wat_path: &Path) -> Option<GpuAbi> {
    let abi_path = abi_json_path(wat_path);
    let text = fs::read_to_string(&abi_path).ok()?;
    let file: AbiFile = serde_json::from_str(&text).ok()?;
    file.gpu
}
