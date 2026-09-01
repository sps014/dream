//! Human-readable `ExitStatus` fragments for error messages.

use std::process::ExitStatus;

/// e.g. `exit code 1` or `terminated by signal` (never Debug-prints `Option`).
pub fn describe(status: &ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(sig) = status.signal() {
                    return format!("terminated by signal {sig}");
                }
            }
            "terminated by signal".to_string()
        }
    }
}
