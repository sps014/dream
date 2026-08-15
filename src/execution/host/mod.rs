//! Wasmtime host glue shared between the CLI runtime ([`super::wasm_runner`]) and the E2E test
//! harness (`tests/e2e_tests.rs`). Both link against the same `env`/`Dream` imports; only the
//! output sink differs (real stdout vs. a captured buffer).
//!
//! The pieces are split by concern so each capability lives next to the stdlib module it backs:
//!   * [`memory`]   - shared string/`char[]` marshaling across the WASM boundary.
//!   * [`file`]     - `src/stdlib/io/file.dream` (synchronous `std::fs`).
//!   * [`http`]     - `src/stdlib/net/http_client.dream` (blocking `reqwest` + the async future bridge).
//!   * [`crypto`]   - `system.crypto` digests and CSPRNG (`sha2` / `hmac` / `getrandom`).
//!   * [`math`]     - the `Math.*` `env` builtins.
//!   * [`console`]  - `src/stdlib/system/system.dream`'s `readLine`/`readKey`/`exit` (the `crossterm` crate).
//!   * [`datetime`] - `src/stdlib/system/datetime.dream`'s wall clock + local timezone offset (the `chrono` crate).

#[cfg(feature = "c-ffi")]
mod c_ffi;
mod console;
mod crypto;
mod datetime;
#[cfg(feature = "c-ffi")]
mod ffi_helpers;
mod file;
mod gpu;
mod http;
mod math;
mod memory;
mod net;
mod process;
mod shared_memory;
mod stack_size;
mod text;
mod webview;
mod worker;

#[cfg(feature = "c-ffi")]
pub use c_ffi::{attach_c_abi_from_json, attach_c_abi_from_wat_path, link_c_ffi_imports};
pub use console::{enable_ansi_support, link_console_functions};
pub use crypto::link_crypto_functions;
pub use datetime::link_datetime_functions;
#[cfg(feature = "c-ffi")]
pub use ffi_helpers::link_ffi_helpers;
pub use file::link_file_functions;
pub use gpu::{attach_abi_from_wat_path, link_gpu_functions, set_packaged_app_icon};
pub use http::link_http_functions;
pub use math::link_math_functions;
pub use memory::{
    read_string_from_memory, shared_bytes, shared_bytes_mut, write_bytes_to_memory,
    write_string_to_memory,
};
pub use net::link_net_functions;
pub use process::link_process_functions;
pub use shared_memory::{shared_memory_for, threaded_wasm_config};
pub use stack_size::{dream_async_stack_size, dream_stack_size, parse_size_bytes};
pub use text::link_text_functions;
pub use webview::link_webview_functions;
pub use worker::{
    link_worker_functions, set_worker_debug, set_worker_module, set_worker_runtime, WorkerDebug,
};

#[cfg(test)]
mod contract_tests {
    //! Guards the `@js("Dream", …)` link contract: every native host function registered under the
    //! `Dream` module must be declared by some stdlib prelude bridge, so a rename/removal on one
    //! side can't silently orphan the other. The reverse direction is intentionally *not* checked:
    //! the dynamic `js` interop names (`jsObject`, `jsCallV`, …) are implemented only by the JS host
    //! (`runtime/dream.js`) and trap under wasmtime, so they have no native registration. That
    //! JS-side mirror remains an untested contract maintained by hand.

    use dream_abi::js_abi::HOST_MODULE;
    use std::collections::HashSet;

    /// Source of every host `.rs` that registers `Dream`-module functions (via `func_wrap("Dream",
    /// "name", …)`). `math` binds only `env` builtins, so it is omitted.
    const HOST_SOURCES: &[&str] = &[
        include_str!("console.rs"),
        include_str!("crypto.rs"),
        include_str!("datetime.rs"),
        include_str!("file.rs"),
        include_str!("gpu/mod.rs"),
        include_str!("http.rs"),
        include_str!("net.rs"),
        include_str!("process.rs"),
        include_str!("text.rs"),
        include_str!("worker.rs"),
        include_str!("webview/mod.rs"),
    ];

    /// Extracts the field name in each `"<HOST_MODULE>", "<name>"` pair, tolerating the line break
    /// `rustfmt` inserts between the two string literals.
    fn names_after_module(src: &str, module: &str) -> Vec<String> {
        let needle = format!("\"{}\"", module);
        let mut out = Vec::new();
        let mut rest = src;
        while let Some(pos) = rest.find(&needle) {
            let after = &rest[pos + needle.len()..];
            // Skip whitespace and the single `,` separating the module from the field literal.
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

    #[test]
    fn every_native_dream_host_fn_is_declared_in_the_prelude() {
        // Names declared by the prelude via `@js("Dream", "name")`.
        let mut declared: HashSet<String> = HashSet::new();
        for (_, src) in dream_stdlib::all_prelude_files() {
            for name in names_after_module(src, HOST_MODULE) {
                declared.insert(name);
            }
        }

        // Names the native host actually registers under the `Dream` module.
        let mut registered: HashSet<String> = HashSet::new();
        for src in HOST_SOURCES {
            for name in names_after_module(src, HOST_MODULE) {
                registered.insert(name);
            }
        }

        assert!(
            !registered.is_empty(),
            "scanner found no native Dream host functions; the pattern likely drifted"
        );

        let orphaned: Vec<&String> = registered.difference(&declared).collect();
        assert!(
            orphaned.is_empty(),
            "native host functions registered under the `Dream` module have no matching \
             `@js(\"Dream\", …)` declaration in the stdlib prelude: {:?}",
            orphaned
        );
    }

    /// JS host factories that contribute keys to the composed `Dream` module (see
    /// `runtime/src/hosts.js` + `workers.js`). `env.js` binds `env`, not `Dream`.
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

    /// Internal JS-only keys that are not `@js` imports (ABI attach hooks, etc.).
    const JS_HOST_INTERNAL_KEYS: &[&str] = &["__attachGpuAbi"];

    /// Host keys emitted by the WAT backend for `js` handle RC (`$js_retain` / `$js_release`), not
    /// declared as Dream stdlib `@js` bridges — ownership is inserted by the RC pass.
    const COMPILER_EMITTED_JS_RC: &[&str] = &["jsRetain", "jsRelease"];

    /// Extracts keys from the primary host factory object in a source file: the last `return {`
    /// on its own line (`make*Host` pattern) or `const host = {` (GPU). Earlier helper
    /// `return {` blocks (e.g. inside GPU bind-group builders) are ignored. Nested objects use
    /// brace depth; keys are recorded before same-line `{`. Method-shorthand `name(` requires
    /// `{` on the line so call continuations are not mistaken for exports.
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
        let mut declared: HashSet<String> = HashSet::new();
        for (_, src) in dream_stdlib::all_prelude_files() {
            for name in names_after_module(src, HOST_MODULE) {
                declared.insert(name);
            }
        }

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
        // Compiler-emitted RC imports are host-only; they must not appear as orphaned prelude decls.
        prelude_only.retain(|n| !COMPILER_EMITTED_JS_RC.contains(&n.as_str()));
        assert!(
            js_only.is_empty() && prelude_only.is_empty(),
            "JS Dream host keys and prelude `@js(\"Dream\", …)` declarations have drifted.\n\
             JS-only (missing from prelude): {:?}\n\
             prelude-only (missing from JS hosts): {:?}",
            js_only,
            prelude_only
        );
    }
}
