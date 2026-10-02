//! Native `system.desktop` host: dialogs, clipboard, and the system URL/path opener.

pub(crate) mod wire;

#[cfg(feature = "webview")]
pub(crate) mod clipboard;
#[cfg(feature = "webview")]
pub(crate) mod dialog;
#[cfg(feature = "webview")]
pub(crate) mod shell;
