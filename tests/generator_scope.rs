use std::path::Path;
use std::process::{Command, Output};

fn dream(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dream"))
        .current_dir(dir)
        .env(
            "DREAM_BENCH_NONCE",
            format!("{:?}", std::time::Instant::now()),
        )
        .args(args)
        .output()
        .unwrap()
}

fn generator(module: &str, value: i32) -> String {
    format!(
        r#"module {module};
import system.codegen;

@generator
@incremental
@syntax_block
public fun stamp(ctx: GenContext): void {{
    for (let block in ctx.syntax_blocks()) {{
        ctx.replace(block, "{value}");
    }}
    ctx.emit_file("marker.dream", "module generated_{module};\npublic fun marker_{module}(): int {{\n    return {value};\n}}\n");
}}
"#
    )
}

#[test]
fn same_name_generators_resolve_in_their_modules_and_materialize_separately() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("dream.toml"), "[package]\nname = \"scoped\"\n").unwrap();
    std::fs::write(root.join("a.dream"), generator("a", 11)).unwrap();
    std::fs::write(root.join("b.dream"), generator("b", 22)).unwrap();
    for module in ["a", "b"] {
        std::fs::write(root.join(format!("{module}_use.dream")), format!(
            "module {module};\nimport {module};\npublic fun value_{module}(): int {{\n    return stamp {{ contents }};\n}}\n"
        )).unwrap();
    }
    std::fs::write(
        root.join("main.dream"),
        "import system;\nimport a_use;\nimport b_use;\nfun main(): void {\n    System.println(value_a());\n    System.println(value_b());\n}\n",
    )
    .unwrap();

    let run = dream(root, &["run", "main.dream"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).replace('\r', ""),
        "11\n22\n"
    );
    for module in ["a", "b"] {
        let output = root.join(format!("generated/main/{module}/stamp"));
        assert!(output.join("marker.dream").is_file());
        let result: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.join("outputs.json")).unwrap()).unwrap();
        assert_eq!(result["generator"], format!("{module}::stamp"));
    }
    assert!(root.join("target/obj/generators/exe").is_dir());
    assert!(!root.join(".dream/generated").exists());

    let explain = dream(root, &["generate", "main.dream", "--explain", "stamp"]);
    assert!(!explain.status.success());
    let log = String::from_utf8_lossy(&explain.stderr);
    assert!(log.contains("ambiguous generator 'stamp'"), "{log}");
    assert!(
        log.contains("a::stamp") && log.contains("b::stamp"),
        "{log}"
    );
    let explain = dream(root, &["generate", "main.dream", "--explain", "a::stamp"]);
    assert!(
        explain.status.success(),
        "{}",
        String::from_utf8_lossy(&explain.stderr)
    );

    let output = root.join("generated/main/a/stamp/marker.dream");
    let modified = std::fs::metadata(&output).unwrap().modified().unwrap();
    let stale = root.join("generated/main/a/stamp/stale.dream");
    std::fs::write(&stale, "stale").unwrap();
    let run = dream(root, &["run", "main.dream"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::metadata(output).unwrap().modified().unwrap(),
        modified
    );
    assert!(!stale.exists());
}

#[test]
fn syntax_generator_imports_must_be_unambiguous() {
    let dir = tempfile::tempdir().unwrap();
    for (module, value) in [("a", 11), ("b", 22)] {
        std::fs::write(
            dir.path().join(format!("{module}.dream")),
            generator(module, value),
        )
        .unwrap();
    }
    std::fs::write(
        dir.path().join("main.dream"),
        "import a;\nimport b;\nfun main(): int {\n    return stamp { contents };\n}\n",
    )
    .unwrap();
    let result = dream(dir.path(), &["generate", "main.dream", "--list"]);
    // Inspection resolves ownership but does not require syntax sites to be expanded.
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = dream(dir.path(), &["main.dream"]);
    assert!(!result.status.success());
    let log = String::from_utf8_lossy(&result.stderr);
    assert!(log.contains("no unambiguous visible generator"), "{log}");
}

#[test]
fn triggers_use_the_generator_module_and_std_names_cannot_be_shadowed() {
    let dir = tempfile::tempdir().unwrap();
    for module in ["a", "b"] {
        std::fs::write(
            dir.path().join(format!("{module}.dream")),
            format!(
                r#"module {module};
import system.codegen;
@attribute(AttributeTarget.Class)
public struct MarkAttribute {{}}
@generator
@on_attribute(MarkAttribute)
@on_call(trigger)
public fun derive(ctx: GenContext): void {{
    ctx.log("ready");
}}
public fun trigger(): int {{
    return 1;
}}
"#
            ),
        )
        .unwrap();
    }
    std::fs::write(
        dir.path().join("shadow.dream"),
        "module shadow;\n@attribute(AttributeTarget.Class)\npublic struct JsonAttribute {}\n",
    )
    .unwrap();
    let main = dir.path().join("main.dream");
    std::fs::write(
        &main,
        "import a;\nimport b;\nimport shadow;\nimport system.json;\nfun main(): void {}\n",
    )
    .unwrap();
    let compiler = dream::driver::compiler::Compiler::new(dream_mir::backend::Target::native());
    let inspection = compiler.inspect_generators(main.to_str().unwrap()).unwrap();
    for module in ["a", "b"] {
        let generator = &inspection
            .find(&format!("{module}::derive"))
            .unwrap()
            .registered;
        assert_eq!(
            generator.attribute_triggers,
            [format!("{module}::MarkAttribute")]
        );
        assert_eq!(generator.call_triggers[0].id, format!("{module}::trigger"));
    }
    assert_eq!(
        inspection
            .find("json_derive")
            .unwrap()
            .registered
            .attribute_triggers,
        ["system.json::JsonAttribute"]
    );
    assert!(inspection.find("derive").is_none());
}

#[test]
fn private_syntax_generators_do_not_become_visible_through_an_import() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("a.dream"),
        generator("a", 11).replace("public fun stamp", "fun stamp"),
    )
    .unwrap();
    let main = dir.path().join("main.dream");
    std::fs::write(
        &main,
        "import a;\nfun main(): int {\n    return stamp { contents };\n}\n",
    )
    .unwrap();
    let compiler = dream::driver::compiler::Compiler::new(dream_mir::backend::Target::native());
    let inspection = compiler.inspect_generators(main.to_str().unwrap()).unwrap();
    let generator = inspection.find("a::stamp").unwrap();
    assert!(!generator.applicable);
    assert_eq!(generator.sites, 0);
}

#[test]
fn generated_extensions_keep_member_access_without_global_private_access() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dream.toml"),
        "[package]\nname = \"visibility\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("model.dream"),
        r#"module model;
public class Model {
    value: int;
    public constructor() {
        this.value = 7;
    }
}
fun secret(): int {
    return 99;
}
"#,
    )
    .unwrap();
    let source = |body: &str| {
        format!(
            r#"module gen;
import system.codegen;
@generator
@syntax_block
public fun stamp(ctx: GenContext): void {{
    for (let block in ctx.syntax_blocks()) {{
        ctx.replace(block, "0");
    }}
    ctx.emit_file("extension.dream", "module model;\nextend Model {{\n    public fun read(): int {{\n        return {body};\n    }}\n}}\n");
}}
"#
        )
    };
    std::fs::write(dir.path().join("gen.dream"), source("this.value")).unwrap();
    std::fs::write(dir.path().join("main.dream"), "import system;\nimport model;\nimport gen;\nfun main(): void {\n    let ignored = stamp { input };\n    System.println(Model().read());\n}\n").unwrap();
    let run = dream(dir.path(), &["run", "main.dream"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "7");

    std::fs::write(dir.path().join("gen.dream"), source("secret()")).unwrap();
    let run = dream(dir.path(), &["main.dream"]);
    assert!(!run.status.success());
    let log = String::from_utf8_lossy(&run.stderr);
    assert!(log.contains("secret") && log.contains("private"), "{log}");

    let main = dir.path().join("main.dream");
    let program = std::fs::read_to_string(&main).unwrap();
    std::fs::write(&main, format!("import system.json;\n{program}")).unwrap();
    std::fs::write(
        dir.path().join("gen.dream"),
        source("JsonParser(\\\"\\\", 0).pos"),
    )
    .unwrap();
    let run = dream(dir.path(), &["main.dream"]);
    assert!(!run.status.success());
    let log = String::from_utf8_lossy(&run.stderr);
    assert!(
        log.contains("JsonParser") && log.contains("not 'public'"),
        "{log}"
    );
}
