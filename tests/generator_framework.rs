//! Generator framework end to end through the CLI: pinned-tool registry and project pins with
//! install roots, parallel tool phases, opaque outputs, module attributes, `@generator_only`
//! declarations and check-only consumed functions.

use std::path::Path;
use std::process::{Command, Output};

const KERNELS_GEN: &str = include_str!("cases/support/generator_framework/kernels/gen.dream");
const KERNELS_SHADERS: &str =
    include_str!("cases/support/generator_framework/kernels/shaders.dream");
const KERNELS_MAIN: &str = include_str!("cases/support/generator_framework/kernels/main.dream");

fn dream(dir: &Path, envs: &[(&str, &Path)], args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dream"));
    command.current_dir(dir).env(
        "DREAM_BENCH_NONCE",
        format!("{:?}", std::time::Instant::now()),
    );
    for (key, value) in envs {
        command.env(key, value);
    }
    command.args(args).output().unwrap()
}

#[cfg(unix)]
fn files_under(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn module_attributes_generator_only_names_and_check_only_consumption() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("dream.toml"),
        "[package]\nname = \"kernels\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("gen.dream"), KERNELS_GEN).unwrap();
    std::fs::write(dir.path().join("shaders.dream"), KERNELS_SHADERS).unwrap();
    std::fs::write(dir.path().join("main.dream"), KERNELS_MAIN).unwrap();
    let run = || dream(dir.path(), &[], &["run", "main.dream"]);

    let ok = run();
    let log = String::from_utf8_lossy(&ok.stderr);
    assert!(ok.status.success(), "{log}");
    let facts = String::from_utf8_lossy(&ok.stdout);
    for expected in [
        "module shaders;",
        "saxpy workgroup=128;",
        "accumulate workgroup=64; refparam total;",
        "saxpy_grad workgroup=64;",
        "call global_id sym=gen::global_id ty=uint;",
        "call lane sym=gen::lane ty=Lane<float>;",
        "call read sym=gen::Lane.read ty=float;",
        "call backward sym=gen::backward ty=float;",
        "ident saxpy sym=shaders::saxpy ty=fun(float, float): float;",
        "ref  sym= ty=float; ident gradient sym=gradient ty=float;",
    ] {
        assert!(facts.contains(expected), "missing `{expected}` in {facts}");
    }

    std::fs::write(
        dir.path().join("main.dream"),
        "import system;\nimport shaders;\nimport gen;\n\nfun main(): void {\n    let id = global_id();\n    System.println(saxpy(1.0, 2.0));\n}\n",
    )
    .unwrap();
    let misuse = run();
    let log = String::from_utf8_lossy(&misuse.stderr);
    assert!(!misuse.status.success(), "{log}");
    assert!(
        log.contains("'global_id' is only available inside functions marked @kernel"),
        "{log}"
    );
    assert!(
        log.contains(
            "'saxpy' was consumed by generator 'gen::kernels' and is not available as a function"
        ),
        "{log}"
    );
    assert!(
        log.contains("help: dispatch it with Kernels.run(\"saxpy\")"),
        "{log}"
    );

    std::fs::write(dir.path().join("main.dream"), KERNELS_MAIN).unwrap();
    std::fs::write(
        dir.path().join("shaders.dream"),
        KERNELS_SHADERS.replace("return a * x + value;", "return a * x + missing_value;"),
    )
    .unwrap();
    let typo = run();
    let log = String::from_utf8_lossy(&typo.stderr);
    assert!(!typo.status.success(), "{log}");
    assert_eq!(
        log.matches("variable missing_value does not exist").count(),
        1,
        "{log}"
    );
}

#[cfg(unix)]
#[test]
fn registry_and_pinned_tools_run_in_parallel_with_opaque_outputs() {
    use std::os::unix::fs::PermissionsExt;
    if cfg!(target_os = "linux") && Command::new("bwrap").arg("--version").output().is_err() {
        eprintln!("sandboxed tool test requires bubblewrap");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let executable = |path: &Path, script: &[u8]| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    let toolchains = root.path().join("toolchains");
    let slang = toolchains.join(format!("slang-{}", dream_abi::toolchain::SLANG_VERSION));
    executable(
        &slang.join("bin/slangc"),
        include_bytes!("cases/support/generator_framework/tools/slangc.sh"),
    );
    std::fs::create_dir_all(slang.join("lib")).unwrap();
    std::fs::write(slang.join("lib/sibling.txt"), "v1").unwrap();

    let project = root.path().join("project");
    let pinned_script = include_bytes!("cases/support/generator_framework/tools/pinned.sh");
    executable(&project.join("tools/pinned/bin/pinned"), pinned_script);
    std::fs::create_dir_all(project.join("tools/pinned/share")).unwrap();
    std::fs::write(project.join("tools/pinned/share/data.txt"), "data1").unwrap();
    std::fs::write(
        project.join("dream.toml"),
        format!(
            "[package]\nname = \"tools\"\n\n[tools.pinned]\npath = \"tools/pinned/bin/pinned\"\nversion = \"1\"\nhash = \"{}\"\nroot = \"tools/pinned\"\ntimeout_secs = 20\n",
            blake3::hash(pinned_script).to_hex()
        ),
    )
    .unwrap();
    std::fs::write(
        project.join("gen.dream"),
        include_str!("cases/support/generator_framework/tools/gen.dream"),
    )
    .unwrap();
    std::fs::write(
        project.join("main.dream"),
        include_str!("cases/support/generator_framework/tools/main.dream"),
    )
    .unwrap();
    let envs = [("DREAM_TOOLCHAINS", toolchains.as_path())];
    let run = || dream(&project, &envs, &["-v", "run", "main.dream"]);
    let expected = |version: &str| {
        format!(
            "slow[slow.txt=slow:payload:{version}]medium[medium.txt=medium:payload:{version}]fast[fast.txt=fast:payload:{version}]pinned[pinned.txt=data1blob.bin opaque 1048576 0]\n1048576"
        )
    };

    for iteration in 0..2 {
        let result = run();
        let log = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "{log}");
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).trim(),
            expected("v1")
        );
        if iteration == 1 {
            assert!(log.contains("gen compiled: result hit"), "{log}");
        }
    }

    let manifest = project.join("generated/main/gen/compiled/outputs.json");
    let transcript: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    let order: Vec<_> = transcript["phases"][1]["tool_results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(order, ["slow", "medium", "fast", "pinned"]);
    assert!(transcript["phases"][0].get("snapshot").is_none());
    assert_eq!(transcript["blobs"].as_object().unwrap().len(), 5);
    let base64_payload = 1048576 * 4 / 3;
    let size = std::fs::metadata(&manifest).unwrap().len() as usize;
    assert!(
        size < base64_payload + 64 * 1024,
        "transcript stores the opaque output more than once: {size} bytes"
    );
    let mut files = Vec::new();
    files_under(&project, &mut files);
    let published = files
        .iter()
        .find(|p| p.ends_with("published/blob.bin"))
        .expect("published tool output");
    assert_eq!(std::fs::metadata(published).unwrap().len(), 1048576);

    std::fs::write(slang.join("lib/sibling.txt"), "v2").unwrap();
    let changed = run();
    let log = String::from_utf8_lossy(&changed.stderr);
    assert!(changed.status.success(), "{log}");
    assert!(log.contains("gen compiled: run "), "{log}");
    assert!(!log.contains("gen compiled: result hit"), "{log}");
    assert_eq!(
        String::from_utf8_lossy(&changed.stdout).trim(),
        expected("v2")
    );

    if Command::new("node").arg("--version").output().is_ok() {
        std::fs::write(project.join("package.json"), "{\"type\":\"module\"}").unwrap();
        let compiled = dream(
            &project,
            &envs,
            &[
                "--wasm",
                "--runtime",
                "--node",
                "-o",
                "opaque.wat",
                "main.dream",
            ],
        );
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        std::fs::write(
            project.join("run.mjs"),
            "import {run, embeddedAssets} from './opaque.node.runtime.js';\nif (embeddedAssets.blob.length !== 1048576 || embeddedAssets.blob[0] !== 255) throw new Error('missing opaque asset');\nawait run('./opaque.wasm', {stdout: s => process.stdout.write(s)});\n",
        )
        .unwrap();
        let node = Command::new("node")
            .arg("run.mjs")
            .current_dir(&project)
            .output()
            .unwrap();
        assert!(
            node.status.success(),
            "{}",
            String::from_utf8_lossy(&node.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&node.stdout).trim(), expected("v2"));
    }
}
