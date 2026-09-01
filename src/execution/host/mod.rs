//! Host implementations for the native C runtime (`libdream`): GPU, HTTP, net, process, crypto, tz, WebView.
//! Guest file/console helpers live in `runtime/c/native/`; GPU/HTTP/net/process stay here so they are
//! the same on every OS (`-ldream`).

mod c_link;
pub(crate) mod crypto;
pub(crate) mod gpu;
pub(crate) mod http;
pub(crate) mod http_server;
pub(crate) mod net;
pub(crate) mod process_host;
pub(crate) mod tz;
#[cfg(feature = "webview")]
pub(crate) mod webview;
#[cfg(not(feature = "webview"))]
#[path = "webview/unavailable.rs"]
pub(crate) mod webview;

pub use c_link::{
    cc_link_flags, find_library_path, read_c_libs_from_abi, search_roots_for_artifact,
};
pub use gpu::{attach_abi_from_wat_path, set_packaged_app_icon};

#[cfg(test)]
mod contract_tests {
    //! Native C ABI (`native_c/abi.rs` + `webview.rs`) vs stdlib `@runtime("…")` names.

    use dream_abi::js_abi::HOST_MODULE;
    use std::collections::HashSet;

    const HOST_SOURCES: &[&str] = &[
        include_str!("../native_c/abi.rs"),
        include_str!("../native_c/webview.rs"),
    ];

    fn names_after_module(src: &str, module: &str) -> Vec<String> {
        let needle = format!("\"{}\"", module);
        let mut out = Vec::new();
        let mut rest = src;
        while let Some(pos) = rest.find(&needle) {
            let after = &rest[pos + needle.len()..];
            let trimmed = after.trim_start_matches([' ', '\n', '\r', '\t', ',']);
            if let Some(field) = trimmed.strip_prefix('"') {
                if let Some(end) = field.find('"') {
                    out.push(field[..end].to_string());
                }
            }
            rest = after;
        }
        out
    }

    fn runtime_attr_names(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = src;
        while let Some(pos) = rest.find("@runtime(") {
            let after = &rest[pos + "@runtime(".len()..];
            let trimmed = after.trim_start();
            if let Some(field) = trimmed.strip_prefix('"') {
                if let Some(end) = field.find('"') {
                    out.push(field[..end].to_string());
                }
            }
            rest = after;
        }
        out
    }

    fn prelude_dream_host_names() -> HashSet<String> {
        let mut declared: HashSet<String> = HashSet::new();
        for (_, src) in dream_stdlib::all_prelude_files() {
            for name in names_after_module(src, HOST_MODULE) {
                declared.insert(name);
            }
            for name in runtime_attr_names(src) {
                declared.insert(name);
            }
        }
        declared
    }

    fn c_abi_fn_names(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in src.lines() {
            let t = line.trim();
            let t = t
                .strip_prefix("pub unsafe extern \"C\" fn ")
                .or_else(|| t.strip_prefix("pub extern \"C\" fn "));
            let Some(t) = t else { continue };
            if let Some(end) = t.find('(') {
                let name = t[..end].trim();
                // `<Host>Async` fns are the deferred variants of an existing `@runtime`
                // host (same wire format, plus a leading future arg), not standalone
                // entry points — they share their base name's prelude declaration.
                if !name.is_empty() && name != "dream_host_bind" && !name.ends_with("Async") {
                    out.push(name.to_string());
                }
            }
        }
        out
    }

    #[test]
    fn every_native_dream_host_fn_is_declared_in_the_prelude() {
        let declared = prelude_dream_host_names();
        let mut registered: HashSet<String> = HashSet::new();
        for src in HOST_SOURCES {
            registered.extend(c_abi_fn_names(src));
        }

        assert!(
            !registered.is_empty(),
            "scanner found no native Dream host functions; the pattern likely drifted"
        );

        let orphaned: Vec<&String> = registered.difference(&declared).collect();
        assert!(
            orphaned.is_empty(),
            "native C host functions have no matching `@runtime(\"…\")` / `@js(\"Dream\", …)` declaration in the stdlib prelude: {:?}",
            orphaned
        );
    }

    const JS_HOST_SOURCES: &[&str] = &[
        include_str!("../../../runtime/src/hosts/js.js"),
        include_str!("../../../runtime/src/hosts/http.js"),
        include_str!("../../../runtime/src/hosts/fs.js"),
        include_str!("../../../runtime/src/hosts/crypto.js"),
        include_str!("../../../runtime/src/hosts/gpu.js"),
        include_str!("../../../runtime/src/hosts/console_process.js"),
        include_str!("../../../runtime/src/hosts/datetime_text.js"),
        include_str!("../../../runtime/src/hosts/net_sockets.js"),
        include_str!("../../../runtime/src/hosts/webview.js"),
        include_str!("../../../runtime/src/workers.js"),
    ];

    const JS_HOST_INTERNAL_KEYS: &[&str] = &["__attachGpuAbi"];
    // Satisfied by the runtime C archive (runtime/c/weak.c + wasm32/weak_stub.c), not by any
    // JS host or libdream ABI table — so they are exempt from the prelude/host parity check.
    const RUNTIME_ARCHIVE_KEYS: &[&str] = &["weakBind", "weakDead", "weakLoad", "weakReleaseRaw"];
    const COMPILER_EMITTED_JS_RC: &[&str] = &["jsRetain", "jsRelease"];
    // Native `system.webapi` (`@native` only; no JS/wasm listen in v1).
    const NATIVE_ONLY_HOST_KEYS: &[&str] = &[
        "httpServerListen",
        "httpServerAccept",
        "httpServerReadBody",
        "httpServerRespond",
        "httpServerShutdown",
        "httpServerWait",
    ];

    fn js_host_export_keys(src: &str) -> HashSet<String> {
        let lines: Vec<&str> = src.lines().collect();
        let mut start = None;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed == "return {"
                || trimmed.starts_with("const host = {")
                || trimmed.starts_with("const host={")
            {
                start = Some(i);
            }
        }
        let Some(start) = start else {
            return HashSet::new();
        };
        let mut keys = HashSet::new();
        let first = lines[start].trim();
        let mut depth = first.chars().filter(|&c| c == '{').count() as i32
            - first.chars().filter(|&c| c == '}').count() as i32;
        for line in &lines[start + 1..] {
            let trimmed = line.trim();
            if depth == 1 {
                let ident = if let Some(r) = trimmed.strip_prefix("async ") {
                    r
                } else {
                    trimmed
                };
                let end = ident
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(ident.len());
                if end > 0 {
                    let name = &ident[..end];
                    let after = ident[end..].trim_start();
                    if after.starts_with(':') || (after.starts_with('(') && trimmed.contains('{')) {
                        keys.insert(name.to_string());
                    }
                }
            }
            for c in trimmed.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            if depth <= 0 {
                break;
            }
        }
        keys
    }

    #[test]
    fn js_dream_host_keys_match_prelude_js_declarations() {
        let declared = prelude_dream_host_names();
        let mut js_keys: HashSet<String> = HashSet::new();
        for src in JS_HOST_SOURCES {
            js_keys.extend(js_host_export_keys(src));
        }
        for internal in JS_HOST_INTERNAL_KEYS {
            js_keys.remove(*internal);
        }
        for emitted in COMPILER_EMITTED_JS_RC {
            js_keys.remove(*emitted);
        }

        assert!(
            !js_keys.is_empty(),
            "scanner found no JS Dream host keys; the pattern likely drifted"
        );

        let js_only: Vec<&String> = js_keys.difference(&declared).collect();
        let mut prelude_only: Vec<&String> = declared.difference(&js_keys).collect();
        prelude_only.retain(|n| !COMPILER_EMITTED_JS_RC.contains(&n.as_str()));
        prelude_only.retain(|n| !RUNTIME_ARCHIVE_KEYS.contains(&n.as_str()));
        prelude_only.retain(|n| !NATIVE_ONLY_HOST_KEYS.contains(&n.as_str()));
        assert!(
            js_only.is_empty() && prelude_only.is_empty(),
            "JS Dream host keys and prelude `@runtime` / `@js(\"Dream\", …)` declarations have drifted.\n\
             JS-only (missing from prelude): {:?}\n\
             prelude-only (missing from JS hosts): {:?}",
            js_only,
            prelude_only
        );
    }
}
