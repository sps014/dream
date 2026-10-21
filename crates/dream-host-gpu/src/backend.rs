//! Strict backend selection. `Auto` takes the first backend in the platform's preference order
//! that this build compiled shaders for and that opens on this machine; an explicit backend is
//! used exactly as asked or fails, and nothing ever falls back to another backend.

use crate::error::{Code, HostError, HostResult};

/// GpuBackend numbering; `Auto` (0) is a request, never a device's backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Backend {
    Dx12 = 1,
    Dx11 = 2,
    Vulkan = 3,
    Metal = 4,
    Cuda = 5,
    WebGpu = 6,
}

impl Backend {
    pub const ALL: [Self; 6] = [
        Self::Dx12,
        Self::Dx11,
        Self::Vulkan,
        Self::Metal,
        Self::Cuda,
        Self::WebGpu,
    ];

    pub fn from_i32(value: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|b| *b as i32 == value)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Dx12 => "Dx12",
            Self::Dx11 => "Dx11",
            Self::Vulkan => "Vulkan",
            Self::Metal => "Metal",
            Self::Cuda => "Cuda",
            Self::WebGpu => "WebGpu",
        }
    }

    /// The `1 << backend` bit of a compiled-backend mask.
    pub fn bit(self) -> u32 {
        1 << (self as i32)
    }
}

/// What `Auto` tries, best first: the platform's native APIs, then WebGPU through Dawn. CUDA
/// only runs compute shaders, so it is chosen only when asked for by name.
pub fn auto_order(os: &str) -> &'static [Backend] {
    match os {
        "windows" => &[
            Backend::Dx12,
            Backend::Vulkan,
            Backend::Dx11,
            Backend::WebGpu,
        ],
        "macos" => &[Backend::Metal, Backend::Vulkan, Backend::WebGpu],
        _ => &[Backend::Vulkan, Backend::WebGpu],
    }
}

/// Picks and opens a backend. `compiled` is the guest's compiled-backend mask (0 for a program
/// without shaders, which may open any backend); `open` tries one backend and explains why it
/// could not.
pub fn select<T>(
    requested: i32,
    compiled: u32,
    order: &[Backend],
    mut open: impl FnMut(Backend) -> Result<T, String>,
) -> HostResult<(Backend, T)> {
    let is_compiled = |b: Backend| compiled == 0 || compiled & b.bit() != 0;
    if requested != 0 {
        let Some(backend) = Backend::from_i32(requested) else {
            return Err(HostError::invalid(format!(
                "{requested} is not a GpuBackend"
            )));
        };
        if !is_compiled(backend) {
            return Err(HostError::new(
                Code::NotCompiled,
                (backend as i32).to_string(),
            ));
        }
        return open(backend)
            .map(|device| (backend, device))
            .map_err(|reason| HostError::new(Code::Unavailable, reason));
    }
    let mut reasons = Vec::new();
    for &backend in order.iter().filter(|b| is_compiled(**b)) {
        match open(backend) {
            Ok(device) => return Ok((backend, device)),
            Err(reason) => reasons.push(format!("{}: {reason}", backend.name())),
        }
    }
    if reasons.is_empty() {
        let names: Vec<_> = Backend::ALL
            .into_iter()
            .filter(|b| compiled & b.bit() != 0)
            .map(Backend::name)
            .collect();
        return Err(HostError::new(
            Code::NoDevice,
            format!(
                "GpuBackend.Auto never picks {}; request it by name",
                names.join(", ")
            ),
        ));
    }
    Err(HostError::new(Code::NoDevice, reasons.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask(backends: &[Backend]) -> u32 {
        backends.iter().map(|b| b.bit()).fold(0, |a, b| a | b)
    }

    #[test]
    fn auto_takes_the_first_compiled_backend_that_opens() {
        let order = auto_order("macos");
        let mut tried = Vec::new();
        let picked = select(0, mask(&[Backend::WebGpu, Backend::Metal]), order, |b| {
            tried.push(b);
            Ok::<_, String>(b)
        })
        .unwrap();
        assert_eq!(picked.0, Backend::Metal);
        assert_eq!(tried, vec![Backend::Metal]);
    }

    #[test]
    fn auto_skips_backends_without_shader_code_and_failed_devices() {
        let order = auto_order("windows");
        let mut tried = Vec::new();
        let picked = select(0, mask(&[Backend::Dx12, Backend::WebGpu]), order, |b| {
            tried.push(b);
            if b == Backend::Dx12 {
                Err("no adapter".to_string())
            } else {
                Ok(b)
            }
        })
        .unwrap();
        assert_eq!(picked.0, Backend::WebGpu);
        assert_eq!(tried, vec![Backend::Dx12, Backend::WebGpu]);
    }

    #[test]
    fn auto_without_shaders_tries_every_platform_backend() {
        let picked = select(0, 0, auto_order("linux"), |b| {
            if b == Backend::Vulkan {
                Err("no ICD".to_string())
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(picked.0, Backend::WebGpu);
    }

    #[test]
    fn auto_reports_every_reason_when_nothing_opens() {
        let error = select(0, 0, auto_order("linux"), |b| {
            Err::<(), _>(format!("{} missing", b.name()))
        })
        .unwrap_err();
        assert_eq!(error.code, Code::NoDevice);
        assert!(error.detail.contains("Vulkan: Vulkan missing"));
        assert!(error.detail.contains("WebGpu: WebGpu missing"));
    }

    #[test]
    fn auto_never_picks_cuda() {
        let error = select(0, mask(&[Backend::Cuda]), auto_order("linux"), |_| {
            Ok::<_, String>(())
        })
        .unwrap_err();
        assert_eq!(error.code, Code::NoDevice);
        assert!(error.detail.contains("Cuda"), "{}", error.detail);
    }

    #[test]
    fn explicit_backends_fail_exactly_and_never_fall_back() {
        let compiled = mask(&[Backend::Vulkan, Backend::WebGpu]);
        let not_compiled = select(4, compiled, &[], |_| Ok::<_, String>(())).unwrap_err();
        assert_eq!(
            (not_compiled.code, not_compiled.detail.as_str()),
            (Code::NotCompiled, "4")
        );

        let mut tried = Vec::new();
        let unavailable = select(3, compiled, auto_order("linux"), |b| {
            tried.push(b);
            Err::<(), _>("no Vulkan driver".to_string())
        })
        .unwrap_err();
        assert_eq!(unavailable.code, Code::Unavailable);
        assert_eq!(unavailable.detail, "no Vulkan driver");
        assert_eq!(tried, vec![Backend::Vulkan], "no fallback to WebGPU");

        assert_eq!(
            select(9, 0, &[], |_| Ok::<_, String>(())).unwrap_err().code,
            Code::Invalid
        );
        let cuda = select(5, mask(&[Backend::Cuda]), &[], Ok::<_, String>).unwrap();
        assert_eq!(cuda.0, Backend::Cuda);
    }
}
