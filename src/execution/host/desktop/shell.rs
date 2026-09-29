//! Open a URL or path with the system's default handler.

pub(crate) fn open(target: &str) -> Result<(), String> {
    if target.trim().is_empty() {
        return Err("Shell.open: empty target".into());
    }
    open::that_detached(target).map_err(|e| format!("Shell.open {target}: {e}"))
}
