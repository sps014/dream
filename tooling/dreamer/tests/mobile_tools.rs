use std::process::Command;

#[test]
fn device_discovery_reports_missing_sdk_as_json_and_failure() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .args(["devices", "--target", "android", "--json"])
        .env("ANDROID_HOME", root.path().join("missing"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["devices"], serde_json::json!([]));
    assert_eq!(report["unavailable"][0]["platform"], "android");
    assert!(
        report["unavailable"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("SDK directory does not exist")
    );
}

#[test]
fn device_discovery_rejects_non_mobile_targets() {
    let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .args(["devices", "--target", "web"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expected android or ios"));
}

#[test]
fn android_doctor_aggregates_missing_sdk_and_compiler_without_installing() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .args(["toolchain", "doctor", "--target", "android", "--json"])
        .env("ANDROID_HOME", root.path().join("missing"))
        .env("DREAM_BIN", root.path().join("missing-compiler"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["platform"], "android");
    assert_eq!(report["healthy"], false);
    let checks = report["checks"].as_array().unwrap();
    for name in [
        "android-sdk",
        "compiler/aarch64-linux-android",
        "compiler/x86_64-linux-android",
    ] {
        let check = checks.iter().find(|check| check["name"] == name).unwrap();
        assert_eq!(check["available"], false);
    }
}

#[cfg(unix)]
#[test]
fn device_discovery_invokes_sdk_adb_and_preserves_unauthorized_devices() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let tools = root.path().join("platform-tools");
    std::fs::create_dir(&tools).unwrap();
    let adb = tools.join("adb");
    std::fs::write(&adb, "#!/bin/sh\n[ \"$1\" = devices ] && [ \"$2\" = -l ] || exit 1\nprintf 'List of devices attached\\nserial unauthorized\\nemulator-5554 device model:Pixel_8\\n'\n").unwrap();
    std::fs::set_permissions(&adb, std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .args(["devices", "--target", "android", "--json"])
        .env("ANDROID_HOME", root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["devices"][0]["id"], "emulator-5554");
    assert_eq!(report["devices"][0]["name"], "Pixel 8");
    assert_eq!(report["devices"][1]["state"], "unauthorized");
    assert!(report["unavailable"].as_array().unwrap().is_empty());
}
