//! Process-lifetime resident high-water mark; child tool memory is measured separately in CI.

pub struct PeakMemoryReport {
    enabled: bool,
}

impl PeakMemoryReport {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }
}

impl Drop for PeakMemoryReport {
    fn drop(&mut self) {
        if self.enabled {
            match peak_resident_bytes() {
                Ok(bytes) => tracing::info!(peak_resident_bytes = bytes, "compiler peak memory"),
                Err(error) => tracing::info!(%error, "compiler peak memory unavailable"),
            }
        }
    }
}

#[cfg(unix)]
fn peak_resident_bytes() -> std::io::Result<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // getrusage initializes the complete structure on success; RUSAGE_SELF excludes children.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let usage = unsafe { usage.assume_init() };
    use std::convert::TryFrom;
    let peak = u64::try_from(usage.ru_maxrss).map_err(|_| {
        std::io::Error::other("getrusage returned a negative resident high-water mark")
    })?;
    #[cfg(target_os = "macos")]
    return Ok(peak);
    #[cfg(not(target_os = "macos"))]
    Ok(peak.saturating_mul(1024))
}

#[cfg(windows)]
fn peak_resident_bytes() -> std::io::Result<u64> {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    let mut counters = std::mem::MaybeUninit::<PROCESS_MEMORY_COUNTERS>::uninit();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // The pseudo-handle is valid for this process and the API writes the supplied sized buffer.
    if unsafe { GetProcessMemoryInfo(GetCurrentProcess(), counters.as_mut_ptr(), size) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { counters.assume_init() }.PeakWorkingSetSize as u64)
}

#[cfg(not(any(unix, windows)))]
fn peak_resident_bytes() -> std::io::Result<u64> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "resident high-water mark is unavailable on this platform",
    ))
}
