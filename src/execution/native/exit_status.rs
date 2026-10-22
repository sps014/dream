//! Classifies a guest's `ExitStatus` as a normal exit or a crash, and renders it for humans.

use std::process::ExitStatus;

/// How a guest process ended abnormally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crash {
    /// Killed by a Unix signal.
    Signal(i32),
    /// Ended by a Windows exception; the exit code is an NTSTATUS error value.
    Exception(u32),
}

impl Crash {
    pub fn of(status: ExitStatus) -> Option<Crash> {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = status.signal() {
                return Some(Crash::Signal(signal));
            }
        }
        // Severity bits `11` mark an NTSTATUS error. Only a negative `main(): int` result could
        // land in that range, and the CLI clamps those to 1 anyway.
        status
            .code()
            .map(|code| code as u32)
            .filter(|&code| code >= 0xC000_0000)
            .map(Crash::Exception)
    }

    /// The exit code a shell reports for this crash: `128 + signal` on Unix, 255 on Windows.
    pub fn exit_code(self) -> u8 {
        match self {
            Crash::Signal(signal) => u8::try_from(128 + signal.clamp(0, 127)).unwrap_or(255),
            Crash::Exception(_) => 255,
        }
    }

    /// Whether the runtime already printed the cause, as `dream_panic` does before aborting.
    pub fn already_reported(self) -> bool {
        match self {
            #[cfg(unix)]
            Crash::Signal(signal) => signal == libc::SIGABRT,
            #[cfg(not(unix))]
            Crash::Signal(_) => false,
            // The Windows CRT's `abort()` ends the process with this fail-fast code.
            Crash::Exception(code) => code == 0xC000_0409,
        }
    }
}

impl std::fmt::Display for Crash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Crash::Signal(signal) => match signal_name(signal) {
                Some(name) => write!(f, "killed by signal {signal} ({name})"),
                None => write!(f, "killed by signal {signal}"),
            },
            Crash::Exception(code) => match exception_name(code) {
                Some(name) => write!(f, "terminated by exception 0x{code:08X} ({name})"),
                None => write!(f, "terminated by exception 0x{code:08X}"),
            },
        }
    }
}

/// `exit code 3`, `killed by signal 11 (SIGSEGV)`, or `terminated by exception 0xC0000005
/// (STATUS_ACCESS_VIOLATION)`.
pub fn describe(status: ExitStatus) -> String {
    if let Some(crash) = Crash::of(status) {
        return crash.to_string();
    }
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => status.to_string(),
    }
}

#[cfg(unix)]
fn signal_name(signal: i32) -> Option<&'static str> {
    Some(match signal {
        libc::SIGHUP => "SIGHUP",
        libc::SIGINT => "SIGINT",
        libc::SIGQUIT => "SIGQUIT",
        libc::SIGILL => "SIGILL",
        libc::SIGTRAP => "SIGTRAP",
        libc::SIGABRT => "SIGABRT",
        libc::SIGBUS => "SIGBUS",
        libc::SIGFPE => "SIGFPE",
        libc::SIGKILL => "SIGKILL",
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGPIPE => "SIGPIPE",
        libc::SIGALRM => "SIGALRM",
        libc::SIGTERM => "SIGTERM",
        libc::SIGSYS => "SIGSYS",
        _ => return None,
    })
}

#[cfg(not(unix))]
fn signal_name(_signal: i32) -> Option<&'static str> {
    None
}

fn exception_name(code: u32) -> Option<&'static str> {
    Some(match code {
        0xC000_0005 => "STATUS_ACCESS_VIOLATION",
        0xC000_001D => "STATUS_ILLEGAL_INSTRUCTION",
        0xC000_008E => "STATUS_FLOAT_DIVIDE_BY_ZERO",
        0xC000_0094 => "STATUS_INTEGER_DIVIDE_BY_ZERO",
        0xC000_0096 => "STATUS_PRIVILEGED_INSTRUCTION",
        0xC000_00FD => "STATUS_STACK_OVERFLOW",
        0xC000_013A => "STATUS_CONTROL_C_EXIT",
        0xC000_0374 => "STATUS_HEAP_CORRUPTION",
        0xC000_0409 => "STATUS_STACK_BUFFER_OVERRUN",
        0xC000_0420 => "STATUS_ASSERTION_FAILURE",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exception_codes_render_with_names() {
        assert_eq!(
            Crash::Exception(0xC000_0005).to_string(),
            "terminated by exception 0xC0000005 (STATUS_ACCESS_VIOLATION)"
        );
        assert_eq!(
            Crash::Exception(0xC123_4567).to_string(),
            "terminated by exception 0xC1234567"
        );
        assert_eq!(Crash::Exception(0xC000_00FD).exit_code(), 255);
        assert!(Crash::Exception(0xC000_0409).already_reported());
        assert!(!Crash::Exception(0xC000_0005).already_reported());
    }

    #[cfg(unix)]
    #[test]
    fn signals_render_with_names_and_shell_exit_codes() {
        let segv = Crash::Signal(libc::SIGSEGV);
        assert_eq!(
            segv.to_string(),
            format!("killed by signal {} (SIGSEGV)", libc::SIGSEGV)
        );
        assert_eq!(segv.exit_code(), 128 + libc::SIGSEGV as u8);
        assert!(Crash::Signal(libc::SIGABRT).already_reported());
        assert!(!segv.already_reported());
    }

    #[cfg(unix)]
    #[test]
    fn describe_distinguishes_exits_from_signals() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(describe(ExitStatus::from_raw(3 << 8)), "exit code 3");
        assert_eq!(
            describe(ExitStatus::from_raw(libc::SIGSEGV)),
            format!("killed by signal {} (SIGSEGV)", libc::SIGSEGV)
        );
    }

    #[cfg(windows)]
    #[test]
    fn describe_names_windows_exceptions() {
        use std::os::windows::process::ExitStatusExt;
        assert_eq!(describe(ExitStatus::from_raw(3)), "exit code 3");
        assert_eq!(
            describe(ExitStatus::from_raw(0xC000_00FD)),
            "terminated by exception 0xC00000FD (STATUS_STACK_OVERFLOW)"
        );
    }
}
