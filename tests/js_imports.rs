use dream::driver::compiler::Compiler;
use dream::driver::js_runtime::JsRuntimeTarget;
use dream_mir::backend::Target;
use std::path::Path;

fn compile(entry: &Path, output: &Path, native: bool) -> Result<(), String> {
    let compiler = if native {
        Compiler::new(Target::native())
    } else {
        Compiler::new(Target::wasm32()).with_runtimes(vec![JsRuntimeTarget::Node])
    };
    compiler
        .with_render_diagnostics(false)
        .compile(
            &entry.to_string_lossy().to_string(),
            &output.to_string_lossy(),
        )
        .map(drop)
        .map_err(|e| e.diagnostic_text().unwrap_or(&e.to_string()).to_string())
}

#[test]
fn imports_preserve_live_handles_callbacks_promises_and_cache_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("main.dream");
    let output = dir.path().join("main.wasm");
    let library = dir.path().join("library.mjs");
    std::fs::write(
        &library,
        r#"
        let item = { name: 'before', count: 1 };
        let previous;
        export function read() { return item; }
        export function apply(callback) {
            if (previous && previous !== callback) throw new Error('callback identity changed');
            previous = callback;
            return callback(7);
        }
        export async function load() { return null; }
        export async function fail() { throw new Error('rejected'); }
        export function tokens(...values) { return values.join(','); }
        export default function add(a, b) { return a + b; }
    "#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("library.d.mts"),
        r#"
        export interface Item { name: string; count: number; }
        export function read(): Item;
        export function apply(callback: (value:number)=>number): number;
        export function load(): Promise<Item|null>;
        export function fail(): Promise<string>;
        export function tokens(...values:string[]): string;
        export default function add(a:number,b:number): number;
    "#,
    )
    .unwrap();
    std::fs::write(
        &entry,
        r#"
        import system;
        jsimport "./library.mjs" as library;
        fun twice(value: double): double { return value * 2; }
        async fun main(): void {
            System.println(library.default(2, 3));
            let first = library.read();
            first.name = "after";
            System.println(library.read().name);
            System.println(library.apply(twice));
            System.println(library.apply(twice));
            System.println(library.tokens("a", "b"));
            switch (library.load().await) {
                Ok(value) => System.println(value.is_none()),
                Err(error) => System.println(error.message),
            }
            switch (library.fail().await) {
                Ok(value) => System.println(value),
                Err(error) => System.println(error.message),
            }
        }
    "#,
    )
    .unwrap();
    compile(&entry, &output, false).unwrap();
    let runner = dir.path().join("run.mjs");
    std::fs::write(
        &runner,
        r#"
        import {run} from './main.node.runtime.js';
        await run(new URL('./main.wasm',import.meta.url), {stdout: value => process.stdout.write(value)});
        "#,
    )
    .unwrap();
    let result = std::process::Command::new("node")
        .arg(&runner)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        "5\nafter\n14\n14\na,b\ntrue\nError: rejected\n"
    );
    let wasm = std::fs::read(&output).unwrap();
    let bundle = std::fs::read(output.with_extension("node.jsimports.mjs")).unwrap();
    compile(&entry, &output, false).unwrap();
    assert_eq!(wasm, std::fs::read(&output).unwrap());
    assert_eq!(
        bundle,
        std::fs::read(output.with_extension("node.jsimports.mjs")).unwrap()
    );
    let changed = std::fs::read_to_string(&library)
        .unwrap()
        .replace("return a + b", "return a - b");
    std::fs::write(&library, changed).unwrap();
    compile(&entry, &output, false).unwrap();
    assert_ne!(
        bundle,
        std::fs::read(output.with_extension("node.jsimports.mjs")).unwrap()
    );
}

#[test]
fn native_js_import_reports_target_error_before_resolving_package() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("main.dream");
    std::fs::write(
        &entry,
        "jsimport \"uninstalled\" as lib; fun main(): void {} ",
    )
    .unwrap();
    let error = compile(&entry, &dir.path().join("main.bin"), true).unwrap_err();
    assert!(
        error.contains("jsimport requires a JavaScript host target"),
        "{error}"
    );
    assert!(!dir.path().join(".dream").exists());
}

#[test]
fn npm_file_dependency_is_installed_and_called() {
    let dir = tempfile::tempdir().unwrap();
    let vendor = dir.path().join("vendor/demo");
    std::fs::create_dir_all(&vendor).unwrap();
    std::fs::write(
        dir.path().join("dream.toml"),
        "[npm]\ndemo = \"file:vendor/demo\"\n",
    )
    .unwrap();
    std::fs::write(
        vendor.join("package.json"),
        r#"{"name":"demo","version":"1.0.0","type":"module","main":"index.js","types":"index.d.ts"}"#,
    )
    .unwrap();
    std::fs::write(
        vendor.join("index.js"),
        "export function greet(name) { return 'hi ' + name; }\n",
    )
    .unwrap();
    std::fs::write(
        vendor.join("index.d.ts"),
        "export function greet(name: string): string;\n",
    )
    .unwrap();
    let entry = dir.path().join("main.dream");
    std::fs::write(
        &entry,
        "import system; jsimport \"demo\" as demo; fun main(): void { System.println(demo.greet(\"Ada\")); }\n",
    )
    .unwrap();
    let output = dir.path().join("main.wasm");
    compile(&entry, &output, false).unwrap();
    let runner = dir.path().join("run.mjs");
    std::fs::write(
        &runner,
        r#"
        import { run } from './main.node.runtime.js';
        await run(new URL('./main.wasm', import.meta.url), { stdout: value => process.stdout.write(value) });
        "#,
    )
    .unwrap();
    let result = std::process::Command::new("node")
        .arg(&runner)
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(String::from_utf8(result.stdout).unwrap(), "hi Ada\n");
}

#[test]
fn published_npm_package_is_installed_and_called() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dream.toml"),
        "[npm]\nescape-string-regexp = \"5.0.0\"\n",
    )
    .unwrap();
    let entry = dir.path().join("main.dream");
    std::fs::write(
        &entry,
        "import system; jsimport \"escape-string-regexp\" as pattern; fun main(): void { System.println(pattern.default(\"a.b\")); }\n",
    )
    .unwrap();
    let output = dir.path().join("main.wasm");
    compile(&entry, &output, false).unwrap();
    let runner = dir.path().join("run.mjs");
    std::fs::write(
        &runner,
        r#"
        import { run } from './main.node.runtime.js';
        await run(new URL('./main.wasm', import.meta.url), { stdout: value => process.stdout.write(value) });
        "#,
    )
    .unwrap();
    let result = std::process::Command::new("node")
        .arg(&runner)
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(String::from_utf8(result.stdout).unwrap(), "a\\.b\n");
}

#[test]
fn generated_members_reject_unknown_readonly_and_captured_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("main.dream");
    std::fs::write(
        dir.path().join("library.mjs"),
        "export function read(){return {name:'test'}} export function each(handler){} ",
    )
    .unwrap();
    std::fs::write(dir.path().join("library.d.mts"), "export function read():{readonly name:string}; export function each(handler:(n:number)=>number):void;").unwrap();
    for (body, expected) in [
        ("library.missing();", "missing"),
        ("let item=library.read(); item.name=\"changed\";", "name"),
        (
            "let offset:double=2; library.each((n:double) => n + offset);",
            "capturing lambdas",
        ),
    ] {
        std::fs::write(
            &entry,
            format!("jsimport \"./library.mjs\" as library; fun main():void {{ {body} }}"),
        )
        .unwrap();
        let error = compile(&entry, &dir.path().join("main.wasm"), false).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}
