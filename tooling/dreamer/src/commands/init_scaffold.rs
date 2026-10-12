//! `dreamer init --test` and `dreamer init --workspace` scaffolds.

use super::init::{Scaffold, write_gitignore};
use crate::manifest::{
    Dependency, DetailedDependency, MANIFEST_FILE_NAME, Manifest, RunTarget, TestConfig, import_segment,
    parse_target_list,
};
use anyhow::{Context, Result, bail};
use std::path::Path;

const EXAMPLE_TEST: &str = "import system;\nimport system.testing;\n\n\
@test\nfun adds_numbers(): void {\n    Assert.eq(1 + 2, 3);\n}\n";

/// A `type = "test"` package: one example test and an empty `fixtures/` asset directory.
/// `runtime` lists the hosts its suite runs on (`native`, `node`).
pub fn test_package(
    dir: &Path,
    name: &str,
    runtime: &[RunTarget],
    depends_on: Option<(&str, &str)>,
) -> Result<()> {
    if runtime.contains(&RunTarget::Web) {
        bail!("test projects run on native and node hosts (web has no process to run tests in)");
    }
    let mut manifest = Manifest::new_test(name.to_string(), "0.1.0".to_string());
    manifest.package_mut()?.assets = vec!["fixtures".to_string()];
    if !runtime.is_empty() {
        manifest.test = Some(TestConfig {
            targets: runtime.iter().map(|t| t.as_str().to_string()).collect(),
        });
    }
    let example = match depends_on {
        Some((dep, path)) => {
            manifest.dependencies.insert(
                dep.to_string(),
                Dependency::Detailed(DetailedDependency {
                    path: Some(path.to_string()),
                    ..DetailedDependency::default()
                }),
            );
            let segment = import_segment(dep);
            format!(
                "import system;\nimport system.testing;\nimport {segment}.greeting;\n\n\
                 @test\nfun greets_by_name(): void {{\n    \
                 Assert.eq(greeting(\"Dream\"), \"Hello, Dream!\");\n}}\n"
            )
        }
        None => EXAMPLE_TEST.to_string(),
    };
    manifest.validate()?;
    std::fs::create_dir_all(dir.join("fixtures"))?;
    write_new(&dir.join("fixtures/.gitkeep"), "")?;
    write_new(&dir.join("example.dream"), &example)?;
    manifest.save(&dir.join(MANIFEST_FILE_NAME))?;
    write_gitignore(dir)?;
    Ok(())
}

/// A virtual workspace with an `app/` bin member and a `tests/` test project that imports it.
pub fn workspace(dir: &Path, name: &str, runtime_spec: Option<String>) -> Result<()> {
    let manifest_path = dir.join(MANIFEST_FILE_NAME);
    if manifest_path.exists() {
        bail!("{} already exists", manifest_path.display());
    }
    let runtime = match &runtime_spec {
        Some(spec) => parse_target_list(spec)?,
        None => Vec::new(),
    };
    Manifest::new_workspace(vec!["app".into(), "tests".into()]).save(&manifest_path)?;
    write_gitignore(dir)?;

    let app = dir.join("app");
    std::fs::create_dir_all(&app)?;
    super::init::run(&app, Some(name.to_string()), runtime_spec, Scaffold::Bin)?;
    let segment = import_segment(name);
    write_new(
        &app.join("src/greeting.dream"),
        "public fun greeting(name: string): string {\n    return \"Hello, \" + name + \"!\";\n}\n",
    )?;
    std::fs::write(
        app.join("src/main.dream"),
        "import system;\nimport greeting;\n\nfun main() {\n    System.println(greeting(\"world\"));\n}\n",
    )
    .with_context(|| format!("writing {}", app.join("src/main.dream").display()))?;

    let tests = dir.join("tests");
    std::fs::create_dir_all(&tests)?;
    let hosts: Vec<RunTarget> = runtime
        .into_iter()
        .filter(|t| *t != RunTarget::Web)
        .collect();
    test_package(
        &tests,
        &format!("{segment}_tests"),
        &hosts,
        Some((name, "../app")),
    )?;
    println!("Created Dream workspace '{name}' at {}", dir.display());
    println!("  app/    the program (`dreamer run -p {name}`)");
    println!("  tests/  its test project (`dreamer test`)");
    Ok(())
}

fn write_new(path: &Path, text: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_scaffold_has_app_and_test_members() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("hello");
        std::fs::create_dir_all(&root).unwrap();
        workspace(&root, "hello", Some("native,node".into())).unwrap();
        let ws = Manifest::load(&root.join(MANIFEST_FILE_NAME)).unwrap();
        assert_eq!(ws.workspace.unwrap().members, vec!["app", "tests"]);
        let app = Manifest::load(&root.join("app").join(MANIFEST_FILE_NAME)).unwrap();
        assert_eq!(app.package().unwrap().targets, vec!["native", "node"]);
        let tests = Manifest::load(&root.join("tests").join(MANIFEST_FILE_NAME)).unwrap();
        tests.validate().unwrap();
        assert_eq!(tests.package().unwrap().assets, vec!["fixtures"]);
        assert_eq!(tests.test.unwrap().targets, vec!["native", "node"]);
        assert!(tests.dependencies.contains_key("hello"));
        let example = std::fs::read_to_string(root.join("tests/example.dream")).unwrap();
        assert!(example.contains("import hello.greeting;"));
        assert!(root.join("tests/fixtures").is_dir());
    }

    #[test]
    fn test_scaffold_rejects_web() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(test_package(tmp.path(), "suite", &[RunTarget::Web], None).is_err());
    }
}
