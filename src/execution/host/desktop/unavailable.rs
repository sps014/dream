//! Stub `system.desktop` host when the `webview` Cargo feature is off (no rfd / clipboard-rs).

const UNAVAILABLE: &str = "system.desktop is not in this Dream build (compiled without `--features webview`)";

pub(crate) mod dialog {
    use super::super::wire::{DialogKind, DialogRequest, Outcome};

    pub(crate) fn start(_kind: DialogKind, _req: &DialogRequest) -> i32 {
        eprintln!("{}", super::UNAVAILABLE);
        0
    }

    pub(crate) fn poll(_handle: i32) -> Option<Outcome> {
        Some(Outcome::Cancelled)
    }
}

pub(crate) mod clipboard {
    use super::super::wire::ClipKind;

    pub(crate) fn get(_kind: ClipKind, _format: &str) -> Option<Vec<u8>> {
        None
    }

    pub(crate) fn set(_kind: ClipKind, _format: &str, _data: &[u8]) -> Result<(), String> {
        Err(super::UNAVAILABLE.into())
    }

    pub(crate) fn has(_format: &str) -> bool {
        false
    }

    pub(crate) fn clear() -> Result<(), String> {
        Err(super::UNAVAILABLE.into())
    }
}

pub(crate) mod shell {
    pub(crate) fn open(_target: &str) -> Result<(), String> {
        Err(super::UNAVAILABLE.into())
    }
}
