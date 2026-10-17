//! Editor support inside generator-consumed functions, replayed from the last build's
//! `.dream/generated/<entry>/<generator>/outputs.json`.

use dream_lsp::analysis::analyze_document;
use dream_lsp::index::Index;
use dream_lsp::sema_ide;
use std::fs;

const GENERATOR: &str = "module gen;\n\nimport system.codegen;\n\n@attribute(AttributeTarget.Function)\npublic struct KernelAttribute {}\n\n@generator_only(KernelAttribute)\npublic extern fun global_id(): uint;\n\n@generator\n@on_attribute(KernelAttribute)\npublic fun shade(ctx: GenContext): void {\n    ctx.log(\"run\");\n}\n";

const MAIN: &str = "import gen;\n\n@kernel\nfun blur(x: float): float {\n    let id = global_id();\n    let doubled = x * 2.0;\n    return doubled;\n}\n\n@kernel\nfun blur_twice(x: float): float {\n    let f = blur;\n    return f(x);\n}\n\nfun main(): void {}\n";

struct Project {
    dir: std::path::PathBuf,
    main: String,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn project(tag: &str) -> Project {
    let dir = std::env::temp_dir().join(format!("dream-lsp-consumed-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("dream.toml"), "[package]\nname = \"kernels\"\n").unwrap();
    fs::write(dir.join("gen.dream"), GENERATOR).unwrap();
    let main = dir.join("main.dream");
    fs::write(&main, MAIN).unwrap();
    let generated = dir.join(".dream/generated/main/shade");
    fs::create_dir_all(&generated).unwrap();
    let artifact = "float blur(float x) {\n    return x * 2.0;\n}\n";
    fs::write(generated.join("blur.slang"), artifact).unwrap();
    let outputs = serde_json::json!([
        {"kind": "consume", "target": "::blur", "help": ""},
        {"kind": "consume", "target": "::blur_twice", "help": ""},
        {
            "kind": "artifact",
            "path": "blur.slang",
            "contents": artifact,
            "mappings": [{"line": 1, "column": 1, "end_line": 3, "end_column": 2, "target": "::blur"}],
            "publish": false
        }
    ]);
    fs::write(
        generated.join("outputs.json"),
        serde_json::to_vec(&serde_json::json!({
            "version": dream::driver::generate::SNAPSHOT_VERSION,
            "generator": "shade",
            "outputs": outputs,
            "diagnostics": [{
                "severity": "error",
                "message": "slang: implicit truncation",
                "artifact": "blur.slang",
                "line": 2,
                "column": 12
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    Project {
        main: main.to_string_lossy().into_owned(),
        dir,
    }
}

fn offset_of(needle: &str, nth: usize) -> usize {
    MAIN.match_indices(needle).nth(nth).unwrap().0
}

#[test]
fn consumed_bodies_are_analyzed_with_generator_only_names() {
    let project = project("hover");
    let outcome = analyze_document(Some(&project.main), MAIN);
    let errors: Vec<_> = outcome
        .diagnostics
        .iter()
        .filter(|d| d.severity == "error")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(errors, ["slang: implicit truncation"], "{errors:?}");
    let snapshot = outcome.sema.expect("semantic snapshot");

    let (_, _, hover) = sema_ide::hover_at(&snapshot, offset_of("global_id", 0)).unwrap();
    assert!(hover.contains("uint"), "{hover}");
    let (_, _, hover) = sema_ide::hover_at(&snapshot, offset_of("doubled", 1)).unwrap();
    assert!(hover.contains("float"), "{hover}");

    let idx = Index::build(Some(&project.main), MAIN);
    let blur_name = offset_of("blur", 0);
    assert_eq!(
        sema_ide::definition_at(&snapshot, &idx, offset_of("blur;", 0)),
        Some((blur_name, blur_name + "blur".len()))
    );
}

#[test]
fn artifact_diagnostics_and_lenses_point_at_the_consumed_function() {
    let project = project("lens");
    let outcome = analyze_document(Some(&project.main), MAIN);
    let diagnostic = outcome
        .diagnostics
        .iter()
        .find(|d| d.message == "slang: implicit truncation")
        .expect("artifact diagnostic");
    assert_eq!(diagnostic.range.start.line, 3);

    let mut consumed = outcome.consumed.clone();
    consumed.sort_by_key(|c| c.start);
    assert_eq!(consumed.len(), 2);
    assert_eq!(consumed[0].start, offset_of("blur", 0));
    assert_eq!(consumed[0].generator, "shade");
    assert!(consumed[0].artifacts[0].ends_with("shade/blur.slang"));
    assert!(consumed[1].artifacts.is_empty());

    let lenses = dream_lsp::backend::consumed_lenses(&consumed, MAIN);
    let titles: Vec<_> = lenses
        .iter()
        .map(|l| l.command.as_ref().unwrap().title.as_str())
        .collect();
    assert_eq!(
        titles,
        [
            "consumed by generator 'shade'",
            "open blur.slang",
            "consumed by generator 'shade'"
        ]
    );
    assert_eq!(lenses[1].command.as_ref().unwrap().command, "vscode.open");
}
