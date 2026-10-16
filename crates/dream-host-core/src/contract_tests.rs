//! Native capability C exports vs stdlib `@runtime("…")` names.

use dream_abi::js_abi::HOST_MODULE;
use std::collections::HashSet;

const HOST_SOURCES: &[&str] = &[
    include_str!("../../dream-host-unicode/src/exports.rs"),
    include_str!("../../dream-host-crypto/src/exports.rs"),
    include_str!("../../dream-host-process/src/exports.rs"),
    include_str!("../../dream-host-timezone/src/exports.rs"),
    include_str!("../../dream-host-net/src/exports.rs"),
    include_str!("../../dream-host-cli/src/exports.rs"),
    include_str!("../../dream-host-data/src/exports.rs"),
    include_str!("../../dream-host-io/src/exports.rs"),
];

fn prelude_dream_host_names(include_native_only: bool) -> HashSet<String> {
    use dream_abi::attributes::{RuntimeSupport, js_import_target, runtime_import_field};
    use dream_syntax::{lexer::Lexer, parser::Parser};
    let mut declared = HashSet::new();
    for (path, source) in dream_stdlib::all_prelude_files() {
        let arena = bumpalo::Bump::new();
        let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
        let mut parser = Parser::new(Lexer::new(source.to_string()), &arena, &mut diagnostics);
        let syntax = parser
            .parse()
            .unwrap_or_else(|_| panic!("invalid stdlib source: {path}"));
        let program = syntax.get_root();
        dream_stdlib::attributes::resolve_source_attribute_ids(program);
        let functions = program
            .functions
            .iter()
            .chain(program.structs.iter().flat_map(|d| &d.methods))
            .chain(program.enums.iter().flat_map(|d| &d.methods))
            .chain(program.extends.iter().flat_map(|d| &d.methods));
        for function in functions {
            let support = RuntimeSupport::from_attributes(&function.attributes);
            if !include_native_only && !support.node && !support.web {
                continue;
            }
            if let Some(field) = runtime_import_field(&function.attributes) {
                declared.insert(field);
            }
            if let Some((module, field)) = js_import_target(&function.attributes)
                && module == HOST_MODULE
            {
                declared.insert(field);
            }
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
            // Deferred-only hosts have no synchronous export; both forms share one import key.
            if !name.is_empty() && name != "dream_host_bind_v2" {
                out.push(name.strip_suffix("Async").unwrap_or(name).to_string());
            }
        }
    }
    out
}

#[test]
fn every_native_dream_host_fn_is_declared_in_the_prelude() {
    let declared = prelude_dream_host_names(true);
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
        (HostCapability::Net, vec![HOST_SOURCES[4]]),
        (HostCapability::Cli, vec![HOST_SOURCES[5]]),
        (HostCapability::Data, vec![HOST_SOURCES[6]]),
        (HostCapability::Io, vec![HOST_SOURCES[7]]),
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
    include_str!("../../../runtime/src/hosts/file_operations.js"),
    include_str!("../../../runtime/src/hosts/cancellation.js"),
    include_str!("../../../runtime/src/hosts/net.js"),
    include_str!("../../../runtime/src/hosts/sockets.js"),
    include_str!("../../../runtime/src/hosts/cli.js"),
    include_str!("../../../runtime/src/hosts/data.js"),
    include_str!("../../../runtime/src/hosts/http.js"),
    include_str!("../../../runtime/src/hosts/crypto.js"),
    include_str!("../../../runtime/src/hosts/console_process.js"),
    include_str!("../../../runtime/src/hosts/process_service.js"),
    include_str!("../../../runtime/src/hosts/datetime_text.js"),
    include_str!("../../../runtime/src/workers.js"),
];

// Satisfied by the runtime C archive (runtime/c/core/weak.c), not by any
// JS host or capability ABI table — so they are exempt from the prelude/host parity check.
const RUNTIME_ARCHIVE_KEYS: &[&str] = &["weakBind", "weakDead", "weakLoad", "weakReleaseRaw"];
// Shared scheduling imports these bridges directly rather than through a stdlib extern.
const RUNTIME_C_JS_IMPORTS: &[&str] = &[
    "cancellationSubscribe",
    "cancellationNotify",
    "asyncHostCancel",
];
const COMPILER_EMITTED_JS_RC: &[&str] = &["jsRetain", "jsRelease"];
// The loader invokes this after guest execution to await worker shutdown; it is not a
// guest import and therefore has no stdlib declaration.
const LOADER_LIFECYCLE_KEYS: &[&str] = &["__drainWorkers"];

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
    let declared = prelude_dream_host_names(false);
    let mut js_keys: HashSet<String> = HashSet::new();
    for src in JS_HOST_SOURCES {
        js_keys.extend(js_host_export_keys(src));
    }
    for emitted in COMPILER_EMITTED_JS_RC
        .iter()
        .chain(LOADER_LIFECYCLE_KEYS)
        .chain(RUNTIME_C_JS_IMPORTS)
    {
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

#[test]
fn runtime_c_imports_have_matching_javascript_bridges() {
    let source = include_str!("../../dream-mir/src/runtime/c/core/include/dream_core.h");
    let javascript =
        js_host_export_keys(include_str!("../../../runtime/src/hosts/cancellation.js"));
    for name in RUNTIME_C_JS_IMPORTS {
        assert!(source.contains(&format!("DREAM_WASM_IMPORT(DREAM_MODULE_HOST, \"{name}\")")));
        assert!(javascript.contains(*name));
    }
}
