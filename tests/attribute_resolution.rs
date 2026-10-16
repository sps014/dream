use std::process::Command;

#[test]
fn typed_attribute_decoders_keep_the_declaring_module() {
    let dir = tempfile::tempdir().unwrap();
    for (file, module, value) in [("left", "one", 7), ("right", "two", 19)] {
        std::fs::write(
            dir.path().join(format!("{file}.dream")),
            format!(
                r#"
module {module};
import system.codegen;
public enum Mode {{ First = {value}, Second }}
@attribute(AttributeTarget.Function)
public struct RouteAttribute {{ public mode: Mode; }}
@route(Mode.First)
public fun sample(): void {{}}
public fun check(): int {{
    if GenAttributes.id<RouteAttribute>() != "{module}::RouteAttribute" {{ return -1; }}
    let args = List<GenAttrArg>();
    args.push(GenAttrArg("enum", "Mode.First", ""));
    let attrs = List<GenAttribute>();
    attrs.push(GenAttribute(GenAttributes.id<RouteAttribute>(), "route", args));
    if GenAttributes.decode<RouteAttribute>(attrs).unwrap().mode == Mode.First {{ return {value}; }}
    return -2;
}}
"#
            ),
        )
        .unwrap();
    }
    std::fs::write(
        dir.path().join("main.dream"),
        r#"
import system.testing;
import left;
import right;
import one.check as left_check;
import two.check as right_check;

fun main(): void {
    Assert.eq(left_check(), 7);
    Assert.eq(right_check(), 19);
}

"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .arg("run")
        .arg(dir.path().join("main.dream"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn declaration_paths_resolve_aliases_and_expand_attribute_only_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("gen.dream"), r#"
module gen;
import system.codegen;
@attribute(AttributeTarget.Function, AttributeTarget.Parameter)
public struct ProbeAttribute { public provider: DeclarationPath; }
@attribute(AttributeTarget.Parameter)
public struct ProviderAttribute { public provider: DeclarationPath; }
@generator
@incremental
@on_attribute(ProbeAttribute)
public fun probe(ctx: GenContext): void {
    for (let f in ctx.functions_with<ProbeAttribute>()) {
        if f.has<ProbeAttribute>() {
            let provider = f.attribute<ProbeAttribute>().unwrap().provider;
            if provider.symbol.is_empty() { ctx.error_at(f.id, "unresolved attribute reference"); }
            if ctx.find_function(provider.symbol).is_none() { ctx.error_at(f.id, "attribute reference missing from closure"); }
        }
    }
}
"#).unwrap();
    std::fs::write(
        dir.path().join("providers.dream"),
        r#"
module providers;
import gen;
public fun nested(): int { return 1; }
public fun left(@provider(nested) x: int): int { return x; }
public fun right(): int { return 2; }
"#,
    )
    .unwrap();
    let mut snapshots = Vec::new();
    for target in ["left", "right"] {
        std::fs::write(
            dir.path().join("main.dream"),
            format!(
                r#"
import gen;
import providers;
import providers.{target} as current_user;
@probe(current_user)
fun handler(): int {{ return 0; }}
fun main(): void {{}}
"#
            ),
        )
        .unwrap();
        let capture = dir.path().join(target);
        let output = Command::new(env!("CARGO_BIN_EXE_dream"))
            .current_dir(dir.path())
            .args(["generate", "--capture", "probe", "-o"])
            .arg(&capture)
            .arg("main.dream")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let snapshot: serde_json::Value =
            serde_json::from_slice(&std::fs::read(capture.join("snapshot.json")).unwrap()).unwrap();
        let handler = snapshot["functions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "handler")
            .unwrap();
        assert_eq!(handler["attributes"][0]["args"][0]["value"], "current_user");
        assert_eq!(
            handler["attributes"][0]["args"][0]["symbol"],
            format!("providers::{target}")
        );
        assert!(
            snapshot["referenced_functions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["id"] == format!("providers::{target}"))
        );
        snapshots.push(snapshot);
    }
    assert_ne!(
        snapshots[0], snapshots[1],
        "alias changes must invalidate incremental results"
    );
    assert!(
        snapshots[0]["referenced_functions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["id"] == "providers::nested")
    );
}

#[test]
fn generated_extensions_preserve_modules_and_import_loaded_function_identities() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("gen.dream"), r#"
module gen;
import system.codegen;
@attribute(AttributeTarget.Class)
public struct ExtendProbeAttribute {}
@generator
@on_attribute(ExtendProbeAttribute)
public fun extend_probe(ctx: GenContext): void {
    for (let declaration in ctx.types_with<ExtendProbeAttribute>()) {
        ctx.emit_extend(declaration, "public static fun generated(): int { return ModuleInternal.answer(); }");
    }
    ctx.emit_file("bridge.dream", "import models.hidden as selected;\npublic fun generated_bridge(): int { return selected(); }\n");
}
"#).unwrap();
    std::fs::write(
        root.path().join("models.dream"),
        r#"
module models;
import gen;
internal class ModuleInternal { internal static fun answer(): int { return 21; } }
fun hidden(): int { return 21; }
@extend_probe
public class Extended {}
"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("main.dream"),
        r#"
import system;
import models;
fun main(): void { System.println(Extended.generated() + generated_bridge()); }
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dream"))
        .current_dir(root.path())
        .args(["run", "main.dream"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
}

#[test]
fn webapi_generated_native_ir_is_deterministic() {
    use dream::{driver::compiler::Compiler, driver::toolchain::ToolchainConfig};
    use dream_mir::backend::Target;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("main.dream");
    let output = root.path().join("main.ll");
    std::fs::write(
        &source,
        r#"
import system.webapi;
@get("/hello/{id}")
fun hello(@path id: int): string { return id.to_string(); }
async fun main(): void {
    WebApp.listen("127.0.0.1", 0).await.unwrap();
    WebApp.shutdown(); WebApp.wait().await;
}
"#,
    )
    .unwrap();
    let compile = || {
        Compiler::new_with_toolchain_config(
            Target::native(),
            std::sync::Arc::new(ToolchainConfig::default()),
        )
        .with_render_diagnostics(false)
        .compile(
            &source.to_string_lossy().into_owned(),
            output.to_str().unwrap(),
        )
        .unwrap();
        std::fs::read(&output).unwrap()
    };
    assert_eq!(compile(), compile());
}
