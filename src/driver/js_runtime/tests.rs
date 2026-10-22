use super::*;

#[test]
fn release_runtime_minification_renames_locals_and_removes_formatting() {
    let source = r#"
        export function load(unnecessarilyLongRuntimeParameter) {
            return globalThis.callback(unnecessarilyLongRuntimeParameter + 1,
                unnecessarilyLongRuntimeParameter * 2);
        }
    "#;
    let text = minify_js_source(source).unwrap();
    assert!(!text.contains("unnecessarilyLongRuntimeParameter"));
    assert!(!text.contains('\n'));
    assert!(text.contains("load"));
    assert_eq!(text, minify_js_source(source).unwrap());
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, &text, oxc_span::SourceType::mjs()).parse();
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn minimal_print_runtime_omits_optional_hosts() {
    let text = assemble_selective_runtime(&[], JsRuntimeTarget::Web).expect("assemble");
    assert!(!text.contains("makeFsHost(getInstance)"));
    assert!(!text.contains("makeCryptoHost(getInstance)"));
    assert!(text.contains("function load("));
    assert!(text.contains("readEmbeddedAbi"));
    assert!(text.contains("const isNode = false;"));
}

#[test]
fn node_target_pins_is_node() {
    let text = assemble_selective_runtime(&[], JsRuntimeTarget::Node).expect("assemble");
    assert!(text.contains("const isNode = true;"));
}

#[test]
fn env_hosts_are_selected_from_the_final_import_inventory() {
    let live = vec![("env".into(), "print_err_char".into())];
    let assembled = assemble_selective_runtime(&live, JsRuntimeTarget::Web).unwrap();
    let text = minify_js_source(&assembled).unwrap();
    assert!(text.contains("print_err_char:"));
    assert!(!text.contains("print_err_string:"));
    assert!(!text.contains("write_stream:"));
    assert!(!text.contains("print_string:"));
}

#[test]
fn synchronous_runtime_omits_async_bridge_but_cancellation_keeps_it() {
    let sync = assemble_runtime(&[], JsRuntimeTarget::Node, false).unwrap();
    assert!(!sync.contains("function wrapAsyncImport("));
    assert!(!sync.contains("const runLoopPumps"));
    let compressed = minify_js_source(&sync).unwrap();
    assert!(!compressed.contains("wrapAsyncImport"));
    assert!(!compressed.contains("awaited JS promise rejected"));
    let async_runtime = assemble_runtime(&[], JsRuntimeTarget::Node, true).unwrap();
    assert!(async_runtime.contains("function wrapAsyncImport("));
    assert!(compressed.len() < minify_js_source(&async_runtime).unwrap().len());
    let cancel = vec![("Dream".into(), "cancellationWait".into())];
    let selected = assemble_runtime(&cancel, JsRuntimeTarget::Web, false).unwrap();
    assert!(selected.contains("function wrapAsyncImport("));
    assert!(selected.contains("const HAS_ASYNC_IMPORTS = true;"));
}

#[test]
fn synchronous_selected_loader_executes_without_async_glue() {
    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let wasm = wat::parse_str(
        r#"(module
            (import "env" "print_err_char" (func $print (param i32)))
            (memory (export "memory") 1)
            (func (export "main") (result i32)
                i32.const 7 call $print i32.const 0)
        )"#,
    )
    .unwrap();
    let live = vec![("env".into(), "print_err_char".into())];
    let requirements = Requirements::from_abi(&serde_json::json!({"externs": []}), &live).unwrap();
    let source = assemble_for_requirements(&live, JsRuntimeTarget::Node, requirements).unwrap();
    for unused in [
        "\n  registerHandle(value) {",
        "\n  callback(index,",
        "\n  readArray(ptr,",
        "\n  __workerInvoke(fnIndex,",
    ] {
        assert!(!source.contains(unused), "retained {unused}");
    }
    let checked = format!(
        "{source}\nconst result = await run(new Uint8Array({wasm:?}), {{abi: {{externs: []}}}});\nif (result.exitCode) throw Error('host call failed');\n"
    );
    for minify in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime.mjs");
        fs::write(
            &path,
            if minify {
                minify_js_source(&checked).unwrap()
            } else {
                checked.clone()
            },
        )
        .unwrap();
        let output = std::process::Command::new("node")
            .arg(path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
}

#[test]
fn compiler_emitted_js_rc_is_bound_from_wasm_imports() {
    let text = assemble_selective_runtime(&[], JsRuntimeTarget::Web).expect("assemble");
    assert!(text.contains(r#"imp.name === "jsRetain" || imp.name === "jsRelease""#));
}

#[test]
fn window_host_joins_only_programs_that_import_it() {
    let without = assemble_selective_runtime(&[], JsRuntimeTarget::Web).unwrap();
    assert!(!without.contains("makeWindowHost(getInstance)"));
    let live = vec![("Dream".into(), "windowPoll".into())];
    let with = assemble_selective_runtime(&live, JsRuntimeTarget::Web).unwrap();
    assert!(with.contains("Object.assign(parts, makeWindowHost(getInstance));"));
}

#[test]
fn gpu_host_joins_only_programs_that_import_it() {
    let without = assemble_selective_runtime(&[], JsRuntimeTarget::Web).unwrap();
    assert!(!without.contains("makeGpuHost(getInstance)"));
    assert!(!without.contains("function gpuParseBundle("));
    let live = vec![
        ("Dream".into(), "gpuDeviceCreate".into()),
        ("Dream".into(), "gpuSubmit".into()),
    ];
    let with = assemble_selective_runtime(&live, JsRuntimeTarget::Web).unwrap();
    assert!(with.contains("Object.assign(parts, makeGpuHost(getInstance));"));
    assert!(with.contains("function gpuParseBundle("));
    assert!(with.contains("function gpuRecordCommands("));
    assert!(!with.contains("makeWindowHost(getInstance)"));
    let manifest = parse_manifest(include_str!("../../../runtime/src/chunks.manifest"));
    assert_eq!(chunk_for_field(&manifest, "gpuSurfaceAcquire"), Some("gpu"));
}

#[test]
fn unreferenced_apis_inside_live_chunks_are_removed() {
    for (field, unused) in [
        ("gpuDeviceCreate", "createComputePipeline"),
        ("jsGlobal", "jsAwait"),
        ("netDomainToAscii", "netClientCreate"),
    ] {
        let live = vec![("Dream".into(), field.into())];
        let assembled = assemble_selective_runtime(&live, JsRuntimeTarget::Web).unwrap();
        let text = minify_js_source(&assembled).unwrap();
        assert!(text.contains(field), "missing {field}");
        assert!(!text.contains(unused), "retained {unused} for {field}");
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_ok()
        {
            let check = match field {
                "jsGlobal" => "if (host.jsGlobal('Math') !== Math) throw Error('global lookup');",
                "netDomainToAscii" => {
                    "if (host.netDomainToAscii('b\\u00fccher.test') !== 'xn--bcher-kva.test') throw Error('IDNA');"
                }
                _ => "if (typeof host.gpuDeviceCreate !== 'function') throw Error('GPU import');",
            };
            let dir = tempfile::tempdir().unwrap();
            let script = dir.path().join("selected.mjs");
            let checked = format!(
                "{assembled}\nconst host=defaultDreamModule(() => ({{}}));\nif(Object.keys(host).join()!=={field:?}) throw Error('unneeded imports: '+Object.keys(host));\n{check}\n"
            );
            fs::write(&script, minify_js_source(&checked).unwrap()).unwrap();
            let output = std::process::Command::new("node")
                .arg(script)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{field}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    let foreign = vec![("application".into(), "gpuDeviceCreate".into())];
    let text = assemble_selective_runtime(&foreign, JsRuntimeTarget::Web).unwrap();
    assert!(!text.contains("makeGpuHost"));
}

#[test]
fn selective_runtime_is_deterministic() {
    let live = vec![
        ("Dream".into(), "jsGlobal".into()),
        ("Dream".into(), "fileRead".into()),
    ];
    let a = assemble_selective_runtime(&live, JsRuntimeTarget::Node).unwrap();
    let b = assemble_selective_runtime(&live, JsRuntimeTarget::Node).unwrap();
    assert_eq!(a, b);
}

#[test]
fn manifest_fields_match_previous_prefix_table() {
    let manifest = parse_manifest(include_str!("../../../runtime/src/chunks.manifest"));
    assert_eq!(chunk_for_field(&manifest, "jsGlobal"), Some("js"));
    assert_eq!(chunk_for_field(&manifest, "fileRead"), Some("fs"));
    assert_eq!(chunk_for_field(&manifest, "dirList"), Some("fs"));
    assert_eq!(chunk_for_field(&manifest, "delayMs"), Some("datetime_text"));
    assert_eq!(chunk_for_field(&manifest, "windowPoll"), Some("window"));
    assert_eq!(chunk_for_field(&manifest, "windowCanvas"), Some("window"));
    assert_eq!(chunk_for_field(&manifest, "delay"), None);
    assert_eq!(chunk_for_field(&manifest, "print_string"), None);
}
