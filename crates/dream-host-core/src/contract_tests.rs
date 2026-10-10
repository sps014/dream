//! Native capability C exports vs stdlib `@runtime("…")` names.

use dream_abi::js_abi::HOST_MODULE;
use std::collections::HashSet;

const HOST_SOURCES: &[&str] = &[
    include_str!("../../dream-host-unicode/src/exports.rs"),
    include_str!("../../dream-host-crypto/src/exports.rs"),
    include_str!("../../dream-host-process/src/exports.rs"),
    include_str!("../../dream-host-timezone/src/exports.rs"),
];

fn names_after_module(src: &str, module: &str) -> Vec<String> {
    let needle = format!("\"{}\"", module);
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(pos) = rest.find(&needle) {
        let after = &rest[pos + needle.len()..];
        let trimmed = after.trim_start_matches([' ', '\n', '\r', '\t', ',']);
        if let Some(field) = trimmed.strip_prefix('"')
            && let Some(end) = field.find('"')
        {
            out.push(field[..end].to_string());
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
        if let Some(field) = trimmed.strip_prefix('"')
            && let Some(end) = field.find('"')
        {
            out.push(field[..end].to_string());
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
            if !name.is_empty() && name != "dream_host_bind_v2" && !name.ends_with("Async") {
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

#[test]
fn capability_inventory_matches_each_library_exports() {
    use dream_abi::host_capability::HostCapability;
    for (capability, sources) in [
        (HostCapability::Unicode, vec![HOST_SOURCES[0]]),
        (HostCapability::Crypto, vec![HOST_SOURCES[1]]),
        (HostCapability::Process, vec![HOST_SOURCES[2]]),
        (HostCapability::Timezone, vec![HOST_SOURCES[3]]),
    ] {
        let exported: HashSet<_> = sources.iter().flat_map(|s| c_abi_fn_names(s)).collect();
        let registered: HashSet<_> = capability.fields().iter().map(|s| s.to_string()).collect();
        assert_eq!(
            exported,
            registered,
            "{} export inventory",
            capability.name()
        );
        for field in capability.fields() {
            assert_eq!(
                HostCapability::for_import(HOST_MODULE, field),
                Some(capability)
            );
        }
    }
}

const JS_HOST_SOURCES: &[&str] = &[
    include_str!("../../../runtime/src/hosts/js.js"),
    include_str!("../../../runtime/src/hosts/fs.js"),
    include_str!("../../../runtime/src/hosts/crypto.js"),
    include_str!("../../../runtime/src/hosts/console_process.js"),
    include_str!("../../../runtime/src/hosts/datetime_text.js"),
    include_str!("../../../runtime/src/workers.js"),
];

// Satisfied by the runtime C archive (runtime/c/core/weak.c), not by any
// JS host or capability ABI table — so they are exempt from the prelude/host parity check.
const RUNTIME_ARCHIVE_KEYS: &[&str] = &["weakBind", "weakDead", "weakLoad", "weakReleaseRaw"];
const COMPILER_EMITTED_JS_RC: &[&str] = &["jsRetain", "jsRelease"];

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
    assert!(
        js_only.is_empty() && prelude_only.is_empty(),
        "JS Dream host keys and prelude `@runtime` / `@js(\"Dream\", …)` declarations have drifted.\n\
         JS-only (missing from prelude): {:?}\n\
         prelude-only (missing from JS hosts): {:?}",
        js_only,
        prelude_only
    );
}
