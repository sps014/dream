use dream_abi::host_capability::HostCapability;
use dream_abi::target::TargetSpec;
use object::Object;
use std::path::Path;
use std::process::Command;

fn assert_no_host_imports(binary: &Path) {
    let bytes = std::fs::read(binary).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    for import in file.imports().unwrap() {
        assert!(!String::from_utf8_lossy(import.library()).contains("dream_host"));
    }
    #[cfg(target_os = "linux")]
    let output = Command::new("readelf")
        .arg("-d")
        .arg(binary)
        .output()
        .unwrap();
    #[cfg(target_os = "macos")]
    let output = Command::new("otool")
        .arg("-L")
        .arg(binary)
        .output()
        .unwrap();
    #[cfg(unix)]
    {
        assert!(output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("dream_host"));
    }
}

#[test]
fn minimal_pack_runs_without_hosts_and_replaces_capability_heavy_pack() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("dream.toml"), "[package]\nname = \"hello\"\nversion = \"0.1.0\"\ntype = \"bin\"\nentry = \"src/main.dream\"\n").unwrap();
    let compiler = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(if cfg!(windows) { "dream.exe" } else { "dream" });
    let target = format!(
        "{}-{}",
        std::env::consts::OS,
        if cfg!(target_arch = "x86_64") {
            "x64"
        } else {
            "arm64"
        }
    );
    let package = project.join("target/pack").join(&target);
    let spec = TargetSpec::host();
    let executable = package.join(format!(
        "hello-{target}{}",
        if cfg!(windows) { ".exe" } else { "" }
    ));
    let source = project.join("src/main.dream");
    let pack = || {
        let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
            .arg("pack")
            .env("DREAM_BIN", &compiler)
            .current_dir(&project)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    std::fs::write(&source, "import system; import system.text; fun main(): void { System.println(Unicode.normalize(\"Hello, world!\", UnicodeNormForm.Nfc)); }").unwrap();
    pack();
    for capability in HostCapability::ALL {
        assert_eq!(
            package.join(capability.library_name(&spec)).is_file(),
            matches!(capability, HostCapability::Core | HostCapability::Unicode)
        );
    }
    for (program, stdout) in [
        ("import system; import system.crypto; import system.process; import system.text; fun main(): void { System.println(\"Hello, world!\"); }", "Hello, world!\n"),
        ("fun main(): void {}", ""),
    ] {
        std::fs::write(&source, program).unwrap();
        pack();
        for capability in HostCapability::ALL {
            assert!(!package.join(capability.library_name(&spec)).exists());
            #[cfg(target_os = "macos")]
            assert!(!package.join("hello.app/Contents/Frameworks").join(capability.library_name(&spec)).exists());
        }
        assert_no_host_imports(&executable);
        let mut run = Command::new(&executable);
        run.env_clear().current_dir(temp.path());
        #[cfg(unix)]
        run.env("PATH", "/usr/bin:/bin");
        #[cfg(windows)]
        {
            let system = std::env::var_os("SystemRoot").unwrap();
            run.env("SystemRoot", &system).env("PATH", Path::new(&system).join("System32"));
        }
        let output = run.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"), stdout);
    }
}
