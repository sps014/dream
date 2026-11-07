//! Debug APK assembly with the SDK's command-line tools (javac, d8, aapt2, zipalign,
//! apksigner), then adb install, launch, and log streaming.

use super::app::Guest;
use super::{capture, sdk};
use crate::app_config::AppConfig;
use crate::compile_flags::CompileFlags;
use crate::workspace::Workspace;
use anyhow::{Context, Result, bail};
use dream_abi::host_capability::HostCapability;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::write::SimpleFileOptions;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/sdl_java.rs"));
}

/// SDL's Java layer uses APIs through this level behind runtime version checks.
pub(super) const COMPILE_API: u32 = 35;
const ACTIVITY: &str = "DreamActivity";

pub(super) fn device_triple(root: &Path, serial: &str) -> Result<&'static str> {
    let abi = capture(Command::new(sdk::adb(root)?).args([
        "-s",
        serial,
        "shell",
        "getprop",
        "ro.product.cpu.abi",
    ]))?;
    triple_for_abi(abi.trim())
}

fn triple_for_abi(abi: &str) -> Result<&'static str> {
    match abi {
        "arm64-v8a" => Ok("aarch64-linux-android"),
        "x86_64" => Ok("x86_64-linux-android"),
        other => bail!("Android ABI {other} is not supported; use an arm64-v8a or x86_64 device"),
    }
}

fn abi_dir(spec: &dream_abi::target::TargetSpec) -> &'static str {
    if spec.triple.to_string().starts_with("aarch64") {
        "arm64-v8a"
    } else {
        "x86_64"
    }
}

struct Tools {
    jar: PathBuf,
    api: u32,
    build_tools: PathBuf,
}

impl Tools {
    fn find(root: &Path) -> Result<Self> {
        let (api, jar) = sdk::android_platform(root, COMPILE_API)?;
        Ok(Self {
            jar,
            api,
            build_tools: sdk::latest_version(&root.join("build-tools"))
                .context("install Android SDK build-tools with sdkmanager")?,
        })
    }

    fn tool(&self, name: &str) -> PathBuf {
        let script = matches!(name, "d8" | "apksigner");
        self.build_tools.join(if script && cfg!(windows) {
            format!("{name}.bat")
        } else {
            sdk::executable(name)
        })
    }
}

pub(super) fn package(
    workspace: &Workspace,
    flags: &CompileFlags,
    app: &AppConfig,
    guest: &Guest,
    root: &Path,
) -> Result<PathBuf> {
    let tools = Tools::find(root)?;
    let package = workspace.manifest.package()?;
    let work = guest.dir.join("apk");
    if work.exists() {
        std::fs::remove_dir_all(&work).with_context(|| format!("clearing {}", work.display()))?;
    }
    let java = work.join("java");
    let classes = work.join("classes");
    std::fs::create_dir_all(&classes)?;

    let mut sources = Vec::new();
    for (path, text) in embedded::SDL_JAVA {
        sources.push(write(&java.join(path), text.as_bytes())?);
    }
    let libraries = load_order(&guest.capabilities);
    sources.push(write(
        &java
            .join(app.id.replace('.', "/"))
            .join(format!("{ACTIVITY}.java")),
        activity(&app.id, &libraries).as_bytes(),
    )?);
    sources.push(write(
        &java.join("org/dream/runtime/Surfaces.java"),
        SURFACES_JAVA.as_bytes(),
    )?);
    run(
        Command::new("javac")
            .args([
                "--release",
                "11",
                "-nowarn",
                "-Xlint:none",
                "-encoding",
                "UTF-8",
            ])
            .arg("-classpath")
            .arg(&tools.jar)
            .arg("-d")
            .arg(&classes)
            .args(&sources),
        "compiling the Android activity",
    )?;
    let dex = work.join("dex");
    std::fs::create_dir_all(&dex)?;
    let mut d8 = Command::new(tools.tool("d8"));
    d8.arg(if flags.compile_profile.is_debug() {
        "--debug"
    } else {
        "--release"
    })
    .args(["--min-api", &app.android_min_sdk.to_string()])
    .arg("--lib")
    .arg(&tools.jar)
    .arg("--output")
    .arg(&dex)
    .args(class_files(&classes)?);
    run(&mut d8, "converting classes to dex")?;

    let manifest = write(
        &work.join("AndroidManifest.xml"),
        manifest(
            app,
            &package.name,
            &package.version,
            tools.api,
            guest.capabilities.contains(&HostCapability::Net),
        )
        .as_bytes(),
    )?;
    let linked = work.join("linked.apk");
    let mut aapt2 = Command::new(tools.tool("aapt2"));
    aapt2
        .arg("link")
        .arg("--manifest")
        .arg(&manifest)
        .arg("-I")
        .arg(&tools.jar)
        .arg("-o")
        .arg(&linked);
    if flags.compile_profile.is_debug() {
        aapt2.arg("--debug-mode");
    }
    run(&mut aapt2, "linking the APK manifest")?;

    let mut entries = vec![(
        "classes.dex".to_string(),
        std::fs::read(dex.join("classes.dex")).context("d8 wrote no classes.dex")?,
        true,
    )];
    let abi = abi_dir(&guest.spec);
    entries.push((
        format!("lib/{abi}/libmain.so"),
        std::fs::read(&guest.artifact)?,
        false,
    ));
    for name in guest.runtime_files() {
        let bytes = std::fs::read(guest.dir.join(&name))
            .with_context(|| format!("reading staged capability file {name}"))?;
        let entry = if name.ends_with(".so") {
            format!("lib/{abi}/{name}")
        } else {
            format!("assets/{name}")
        };
        entries.push((entry, bytes, !name.ends_with(".so")));
    }
    entries.extend(package_assets(&workspace.root, &package.assets, &work)?);
    append(&linked, &entries)?;

    let aligned = work.join("aligned.apk");
    run(
        Command::new(tools.tool("zipalign"))
            .args(["-P", "16", "-f", "4"])
            .arg(&linked)
            .arg(&aligned),
        "aligning the APK",
    )?;
    let keystore = debug_keystore()?;
    let apk = guest.dir.join(format!("{}.apk", package.name));
    run(
        Command::new(tools.tool("apksigner"))
            .arg("sign")
            .arg("--ks")
            .arg(&keystore)
            .args([
                "--ks-pass",
                "pass:android",
                "--key-pass",
                "pass:android",
                "--ks-key-alias",
                "androiddebugkey",
                "--out",
            ])
            .arg(&apk)
            .arg(&aligned),
        "signing the APK with the debug key",
    )?;
    Ok(apk)
}

pub(super) fn install_and_launch(root: &Path, serial: &str, id: &str, apk: &Path) -> Result<()> {
    let adb = sdk::adb(root)?;
    let device = |args: &[&str]| {
        let mut command = Command::new(&adb);
        command.args(["-s", serial]).args(args);
        command
    };
    run(device(&["install", "-r"]).arg(apk), "installing the APK")?;
    // A new install must not inherit the previous launch's log lines.
    let _ = device(&["logcat", "-c"]).output();
    run(
        &mut device(&[
            "shell",
            "am",
            "start",
            "-W",
            "-n",
            &format!("{id}/.{ACTIVITY}"),
        ]),
        "launching the app",
    )?;
    let pid = capture(&mut device(&["shell", "pidof", id]))
        .context("the app exited during launch; inspect `adb logcat`")?;
    let pid = pid.trim();
    println!("{id} is running (pid {pid}); streaming its log, press Ctrl-C to stop");
    let status = device(&["logcat", &format!("--pid={pid}")])
        .status()
        .context("streaming the app log")?;
    if !status.success() {
        bail!(
            "adb logcat failed ({})",
            crate::process_status::describe(&status)
        );
    }
    Ok(())
}

/// Core first, the window host (which registers SDL's Java natives) before anything that opens a
/// window, and the guest last: SDLActivity loads each in order and calls `main` in the last.
fn load_order(capabilities: &[HostCapability]) -> Vec<String> {
    capabilities
        .iter()
        .map(|capability| capability.link_name().to_string())
        .chain(std::iter::once("main".to_string()))
        .collect()
}

const SURFACES_JAVA: &str = r#"package org.dream.runtime;

/** Blocks the UI thread until the window thread has dropped GPU surfaces. */
public class Surfaces {
    public static native void release();
}
"#;

fn activity(id: &str, libraries: &[String]) -> String {
    let libraries = libraries
        .iter()
        .map(|name| format!("            \"{name}\",\n"))
        .collect::<String>();
    format!(
        "package {id};\n\n\
         import android.content.res.AssetManager;\n\
         import android.os.Bundle;\n\
         import android.view.SurfaceHolder;\n\
         import java.io.File;\n\
         import java.io.FileOutputStream;\n\
         import java.io.InputStream;\n\
         import java.io.IOException;\n\
         import org.libsdl.app.SDLActivity;\n\
         import org.libsdl.app.SDLSurface;\n\n\
         public class {ACTIVITY} extends SDLActivity {{\n    \
             @Override\n    \
             protected void onCreate(Bundle savedInstanceState) {{\n        \
                 copyAssets();\n        \
                 super.onCreate(savedInstanceState);\n    \
             }}\n\n    \
             @Override\n    \
             protected SDLSurface createSDLSurface(android.content.Context context) {{\n        \
                 return new DreamSurface(context);\n    \
             }}\n\n    \
             @Override\n    \
             protected String[] getLibraries() {{\n        \
                 return new String[] {{\n{libraries}        }};\n    \
             }}\n\n    \
             @Override\n    \
             protected String getMainFunction() {{\n        \
                 return \"main\";\n    \
             }}\n\n    \
             private void copyAssets() {{\n        \
                 try {{\n            \
                     copyTree(getAssets(), \"\", getFilesDir());\n        \
                 }} catch (IOException error) {{\n            \
                     android.util.Log.e(\"Dream\", \"copying assets failed\", error);\n        \
                 }}\n    \
             }}\n\n    \
             private static void copyTree(AssetManager assets, String path, File root) throws IOException {{\n        \
                 String[] names = assets.list(path);\n        \
                 if (names == null) {{\n            \
                     return;\n        \
                 }}\n        \
                 if (names.length == 0) {{\n            \
                     if (!path.isEmpty()) {{\n                \
                         copyFile(assets, path, new File(root, path));\n            \
                     }}\n            \
                     return;\n        \
                 }}\n        \
                 for (String name : names) {{\n            \
                     String child = path.isEmpty() ? name : path + \"/\" + name;\n            \
                     String[] nested = assets.list(child);\n            \
                     if (nested != null && nested.length > 0) {{\n                \
                         copyTree(assets, child, root);\n            \
                     }} else {{\n                \
                         copyFile(assets, child, new File(root, child));\n            \
                     }}\n        \
                 }}\n    \
             }}\n\n    \
             private static void copyFile(AssetManager assets, String path, File dest) throws IOException {{\n        \
                 File parent = dest.getParentFile();\n        \
                 if (parent != null) {{\n            \
                     parent.mkdirs();\n        \
                 }}\n        \
                 try (InputStream input = assets.open(path); FileOutputStream output = new FileOutputStream(dest)) {{\n            \
                     byte[] buffer = new byte[8192];\n            \
                     int read;\n            \
                     while ((read = input.read(buffer)) >= 0) {{\n                \
                         output.write(buffer, 0, read);\n            \
                     }}\n        \
                 }}\n    \
             }}\n\
         }}\n\n\
         class DreamSurface extends SDLSurface {{\n    \
             public DreamSurface(android.content.Context context) {{\n        \
                 super(context);\n    \
             }}\n\n    \
             @Override\n    \
             public void surfaceDestroyed(SurfaceHolder holder) {{\n        \
                 org.dream.runtime.Surfaces.release();\n        \
                 super.surfaceDestroyed(holder);\n    \
             }}\n\
         }}\n"
    )
}

fn manifest(app: &AppConfig, label: &str, version: &str, target: u32, network: bool) -> String {
    let network = if network {
        "    <uses-permission android:name=\"android.permission.INTERNET\" />\n"
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="{id}"
    android:versionCode="1"
    android:versionName="{version}">
    <uses-sdk android:minSdkVersion="{min}" android:targetSdkVersion="{target}" />
    <uses-feature android:name="android.hardware.touchscreen" android:required="false" />
{network}    <application
        android:label="{label}"
        android:hasCode="true"
        android:extractNativeLibs="false"
        android:hardwareAccelerated="true"
        android:theme="@android:style/Theme.Black.NoTitleBar.Fullscreen">
        <activity
            android:name=".{ACTIVITY}"
            android:exported="true"
            android:launchMode="singleInstance"
            android:alwaysRetainTaskState="true"
            android:configChanges="layoutDirection|locale|grammaticalGender|fontScale|fontWeightAdjustment|orientation|uiMode|screenLayout|screenSize|smallestScreenSize|keyboard|keyboardHidden|navigation">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#,
        id = app.id,
        min = app.android_min_sdk,
        label = xml_escape(label),
        version = xml_escape(version),
    )
}

/// APK `assets/` entries for `[package].assets`, preserving paths under the package root.
fn package_assets(
    root: &Path,
    assets: &[String],
    work: &Path,
) -> Result<Vec<(String, Vec<u8>, bool)>> {
    if assets.is_empty() {
        return Ok(Vec::new());
    }
    let staged = work.join("staged-assets");
    crate::assets::copy_into(root, assets, &staged)?;
    let mut files = Vec::new();
    let mut dirs = vec![staged.clone()];
    while let Some(dir) = dirs.pop() {
        let mut children = std::fs::read_dir(&dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .collect::<std::io::Result<Vec<_>>>()?;
        children.sort_by_key(|entry| entry.file_name());
        for child in children {
            let path = child.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(&staged)
                .expect("staged asset stays under the staging directory")
                .to_string_lossy()
                .replace('\\', "/");
            files.push((
                format!("assets/{relative}"),
                std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?,
                true,
            ));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Native libraries stay uncompressed so the platform maps them from the APK; zipalign then
/// page-aligns them.
fn append(apk: &Path, entries: &[(String, Vec<u8>, bool)]) -> Result<()> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(apk)
        .with_context(|| format!("opening {}", apk.display()))?;
    let mut writer = zip::ZipWriter::new_append(file)?;
    for (name, bytes, compress) in entries {
        let options = SimpleFileOptions::default().compression_method(if *compress {
            zip::CompressionMethod::Deflated
        } else {
            zip::CompressionMethod::Stored
        });
        writer.start_file(name, options)?;
        writer.write_all(bytes)?;
    }
    writer.finish()?;
    Ok(())
}

fn class_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "class") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The SDK's shared debug key, created with the standard parameters when it is missing.
fn debug_keystore() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("locating the home directory")?
        .join(".android");
    let keystore = dir.join("debug.keystore");
    if keystore.is_file() {
        return Ok(keystore);
    }
    std::fs::create_dir_all(&dir)?;
    run(
        Command::new("keytool")
            .args(["-genkeypair", "-keystore"])
            .arg(&keystore)
            .args([
                "-storepass",
                "android",
                "-alias",
                "androiddebugkey",
                "-keypass",
                "android",
                "-keyalg",
                "RSA",
                "-keysize",
                "2048",
                "-validity",
                "10000",
                "-dname",
                "CN=Android Debug,O=Android,C=US",
            ]),
        "creating the Android debug keystore",
    )?;
    Ok(keystore)
}

fn write(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(path.to_path_buf())
}

fn run(command: &mut Command, action: &str) -> Result<()> {
    capture(command).with_context(|| action.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_abis_select_supported_triples() {
        assert_eq!(
            triple_for_abi("arm64-v8a").unwrap(),
            "aarch64-linux-android"
        );
        assert_eq!(triple_for_abi("x86_64").unwrap(), "x86_64-linux-android");
        assert!(triple_for_abi("armeabi-v7a").is_err());
    }

    #[test]
    fn package_assets_keep_their_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("pkg");
        std::fs::create_dir_all(root.join("fixtures")).unwrap();
        std::fs::write(root.join("fixtures/a.txt"), "hi").unwrap();
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let files = package_assets(&root, &["fixtures".into()], &work).unwrap();
        assert_eq!(files[0].0, "assets/fixtures/a.txt");
        assert_eq!(files[0].1, b"hi");
        assert!(package_assets(&root, &[], &work).unwrap().is_empty());
    }

    #[test]
    fn activity_loads_capabilities_in_order_and_starts_main_last() {
        let libraries = load_order(&[
            HostCapability::Core,
            HostCapability::Window,
            HostCapability::Gpu,
        ]);
        assert_eq!(
            libraries,
            [
                "dream_host_core",
                "dream_host_window",
                "dream_host_gpu",
                "main"
            ]
        );
        let source = activity("org.example.counter", &libraries);
        assert!(source.starts_with("package org.example.counter;"));
        assert!(source.contains("extends SDLActivity"));
        assert!(source.contains("\"dream_host_window\",\n            \"dream_host_gpu\""));
        assert!(source.contains("return \"main\";"));
        assert!(source.contains("org.dream.runtime.Surfaces.release()"));
        assert!(source.contains("copyTree(getAssets()"));
    }

    #[test]
    fn manifest_escapes_labels_and_requests_network_only_when_used() {
        let app = AppConfig::new("demo");
        let text = manifest(&app, "a<b>&\"c\"", "1.0.0", 35, false);
        assert!(text.contains("android:label=\"a&lt;b&gt;&amp;&quot;c&quot;\""));
        assert!(text.contains(&format!("package=\"{}\"", app.id)));
        assert!(text.contains("android:minSdkVersion=\"26\" android:targetSdkVersion=\"35\""));
        assert!(!text.contains("INTERNET"));
        assert!(manifest(&app, "demo", "1.0.0", 35, true).contains("android.permission.INTERNET"));
    }

    #[test]
    fn sdl_java_sources_are_embedded() {
        assert!(
            embedded::SDL_JAVA
                .iter()
                .any(|(path, text)| *path == "org/libsdl/app/SDLActivity.java"
                    && text.contains("class SDLActivity"))
        );
    }

    #[test]
    fn appended_native_libraries_are_stored_and_resources_compressed() {
        let dir = tempfile::tempdir().unwrap();
        let apk = dir.path().join("linked.apk");
        {
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&apk).unwrap());
            writer
                .start_file("AndroidManifest.xml", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"manifest").unwrap();
            writer.finish().unwrap();
        }
        append(
            &apk,
            &[
                ("classes.dex".into(), vec![1; 4096], true),
                ("lib/arm64-v8a/libmain.so".into(), vec![2; 4096], false),
            ],
        )
        .unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&apk).unwrap()).unwrap();
        assert!(archive.by_name("AndroidManifest.xml").is_ok());
        assert_eq!(
            archive.by_name("classes.dex").unwrap().compression(),
            zip::CompressionMethod::Deflated
        );
        assert_eq!(
            archive
                .by_name("lib/arm64-v8a/libmain.so")
                .unwrap()
                .compression(),
            zip::CompressionMethod::Stored
        );
    }
}
