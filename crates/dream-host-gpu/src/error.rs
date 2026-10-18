//! Host failures in the `CODE\ndetail` form `gpu_error_from_host` decodes into `GpuError`.

use crate::ffi;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    NoDevice,
    /// Detail is the GpuBackend number.
    NotCompiled,
    Unavailable,
    DeviceLost,
    Invalid,
    WrongDevice,
    /// Detail is the GpuFeature number.
    Unsupported,
    Validation,
    OutOfMemory,
    Surface,
}

impl Code {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoDevice => "ENODEVICE",
            Self::NotCompiled => "ENOTCOMPILED",
            Self::Unavailable => "EUNAVAILABLE",
            Self::DeviceLost => "EDEVICELOST",
            Self::Invalid => "EINVALID",
            Self::WrongDevice => "EWRONGDEVICE",
            Self::Unsupported => "EUNSUPPORTED",
            Self::Validation => "EVALIDATION",
            Self::OutOfMemory => "ENOMEM",
            Self::Surface => "ESURFACE",
        }
    }
}

/// GpuFeature numbering, for `Code::Unsupported` details.
pub mod feature {
    pub const RAY_TRACING: u32 = 0;
    pub const RAY_QUERY: u32 = 1;
    pub const BINDLESS: u32 = 2;
    pub const COOPERATIVE_VECTORS: u32 = 4;

    /// The GpuFeature behind a bundle capability name (docs/internals/18-gpu-shader-bundle.md).
    pub fn of_capability(name: &str) -> Option<u32> {
        Some(match name {
            "raytracing" => RAY_TRACING,
            "rayquery" => RAY_QUERY,
            "descriptor_handle" => BINDLESS,
            "subgroup_basic" => 3,
            "cooperative_vector" => COOPERATIVE_VECTORS,
            "cooperative_matrix" => 5,
            "half" => 6,
            "atomic64" => 7,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    pub code: Code,
    pub detail: String,
}

pub type HostResult<T> = Result<T, HostError>;

impl HostError {
    pub fn new(code: Code, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }

    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::new(Code::Invalid, detail)
    }

    pub fn unsupported(feature: u32) -> Self {
        Self::new(Code::Unsupported, feature.to_string())
    }

    /// The glue's failure `status` on this thread, with its message as the detail.
    pub fn from_status(status: i32) -> Self {
        let code = match status {
            ffi::INVALID => Code::Invalid,
            ffi::OUT_OF_MEMORY => Code::OutOfMemory,
            ffi::DEVICE_LOST => Code::DeviceLost,
            ffi::SURFACE_LOST => Code::Surface,
            ffi::FAILED | ffi::UNSUPPORTED | ffi::TIMEOUT => Code::Validation,
            _ => Code::Validation,
        };
        Self::new(code, ffi::message())
    }

    pub fn text(&self) -> String {
        format!("{}\n{}", self.code.as_str(), self.detail)
    }
}

/// Turns a glue status into a result.
pub fn check(status: i32) -> HostResult<()> {
    if status == ffi::OK {
        Ok(())
    } else {
        Err(HostError::from_status(status))
    }
}

// Process-wide: async failures are recorded on completion threads and read by the guest.
static LAST: Mutex<String> = Mutex::new(String::new());

pub fn record(error: &HostError) {
    if let Ok(mut last) = LAST.lock() {
        *last = error.text();
    }
}

pub fn last_text() -> String {
    LAST.lock().map(|last| last.clone()).unwrap_or_default()
}
