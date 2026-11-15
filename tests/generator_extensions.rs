#![cfg(unix)]

use std::process::Command;

const GENERATOR: &str = include_str!("cases/support/generator_extensions/gen.dream");
#[cfg(unix)]
#[test]
fn tool_feedback_assets_consumption_and_cached_replay() {
    use std::os::unix::fs::PermissionsExt;
    if cfg!(target_os = "linux") && Command::new("bwrap").arg("--version").output().is_err() {
        eprintln!("sandboxed tool test requires bubblewrap");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let tool = dir.path().join("copy-tool");
    let script = include_bytes!("cases/support/generator_extensions/copy.sh");
    std::fs::write(&tool, script).unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(dir.path().join("gen.dream"), GENERATOR).unwrap();
    std::fs::write(dir.path().join("dream.toml"), format!(
        "[package]\nname = \"generator-extensions\"\n[tools.copy]\npath = \"copy-tool\"\nversion = \"1\"\nhash = \"{}\"\n", blake3::hash(script).to_hex()
    )).unwrap();
    let source = include_str!("cases/support/generator_extensions/main.dream");
    std::fs::write(dir.path().join("main.dream"), source).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_dream"))
            .current_dir(dir.path())
            .env(
                "DREAM_BENCH_NONCE",
                format!("{:?}", std::time::Instant::now()),
            )
            .args(["-v", "run", "main.dream"])
            .output()
            .unwrap()
    };
    for iteration in 0..2 {
        let result = run();
        let log = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "{log}");
        assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "3\n65");
        if iteration == 1 {
            assert!(log.contains("gen gen::resources: result hit"), "{log}");
        }
    }
    let mut changed_tool = script.to_vec();
    changed_tool.extend_from_slice(b"\n# changed fingerprint\n");
    std::fs::write(&tool, &changed_tool).unwrap();
    let manifest_path = dir.path().join("dream.toml");
    let pins = std::fs::read_to_string(&manifest_path).unwrap().replace(
        blake3::hash(script).to_hex().as_str(),
        blake3::hash(&changed_tool).to_hex().as_str(),
    );
    std::fs::write(&manifest_path, pins).unwrap();
    let changed = run();
    let changed_log = String::from_utf8_lossy(&changed.stderr);
    assert!(changed.status.success(), "{changed_log}");
    assert!(
        changed_log.contains("gen gen::resources: run "),
        "{changed_log}"
    );
    assert!(
        !changed_log.contains("gen gen::resources: result hit"),
        "{changed_log}"
    );
    if Command::new("node").arg("--version").output().is_ok() {
        std::fs::write(dir.path().join("package.json"), "{\"type\":\"module\"}").unwrap();
        let compiled = Command::new(env!("CARGO_BIN_EXE_dream"))
            .current_dir(dir.path())
            .args([
                "--wasm",
                "--runtime",
                "--node",
                "-o",
                "asset.wat",
                "main.dream",
            ])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        std::fs::write(dir.path().join("run.mjs"), "import {run, embeddedAssets} from './asset.node.runtime.js';\nif (embeddedAssets.payload[0] !== 65) throw new Error('missing JS asset');\nawait run('./asset.wasm', {stdout: s => process.stdout.write(s)});\n").unwrap();
        let node = Command::new("node")
            .arg("run.mjs")
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            node.status.success(),
            "{}",
            String::from_utf8_lossy(&node.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&node.stdout).trim(), "3\n65");
    }
    let manifest = dir.path().join("generated/main/gen/resources/outputs.json");
    let result: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest).unwrap()).unwrap();
    assert_eq!(result["phases"].as_array().unwrap().len(), 2);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("generated/main/gen/resources/input.txt")).unwrap(),
        "ABC"
    );
    for (mode, expected) in [
        ("duplicate", "already consumed by generator"),
        ("body_error", "body marker"),
        ("artifact_error", "artifact marker"),
        ("tool_failure", "intentional tool failure"),
        ("round_cap", "final feedback round"),
    ] {
        std::fs::write(
            dir.path().join("main.dream"),
            source.replace("@resource(\"ok\")", &format!("@resource(\"{mode}\")")),
        )
        .unwrap();
        let result = run();
        let log = String::from_utf8_lossy(&result.stderr);
        assert!(!result.status.success(), "{mode}: {log}");
        assert!(log.contains(expected), "{mode}: {log}");
        if mode == "body_error" {
            assert!(log.contains("let answer = 42"), "{log}");
        }
    }
    std::fs::write(
        dir.path().join("main.dream"),
        source.replace(
            "System.println(bytes.length);",
            "System.println(cpu_only());",
        ),
    )
    .unwrap();
    let result = run();
    let log = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(
        log.contains("was consumed by generator 'gen::resources'"),
        "{log}"
    );
}
