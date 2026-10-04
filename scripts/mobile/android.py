"""Link Dream staticlibs into an AAR and execute its Java bridge in an Android app."""

import argparse
import os
from pathlib import Path
import time
import zipfile

from common import PACKAGE, PASS, SAMPLE, build_slice, capture, prepare, result, run


def build_apk(output, aar, sdk):
    app = output / "app"
    app.mkdir(parents=True, exist_ok=True)
    tools = sdk / "build-tools/34.0.0"
    android_jar = sdk / "platforms/android-34/android.jar"
    generated = app / "java"
    generated.mkdir(exist_ok=True)
    with zipfile.ZipFile(aar) as archive:
        bridge = generated / "DreamLibrary.java"
        bridge.write_bytes(archive.read("sources/DreamLibrary.java"))
        libraries = {
            name.replace("jni/", "lib/", 1): archive.read(name)
            for name in archive.namelist() if name.startswith("jni/") and name.endswith(".so")
        }
    classes = app / "classes"
    classes.mkdir(exist_ok=True)
    unsigned = app / "unsigned.apk"
    run(
        tools / "aapt2", "link", "-o", unsigned, "--manifest",
        SAMPLE / "android/AndroidManifest.xml", "-I", android_jar,
    )
    run(
        "javac", "--release", "8", "-classpath", android_jar, "-d", classes,
        SAMPLE / "android/MainActivity.java", bridge,
    )
    dex = app / "dex"
    dex.mkdir(exist_ok=True)
    run(tools / "d8", "--min-api", "23", "--lib", android_jar, "--output", dex,
        *sorted(classes.rglob("*.class")))
    with zipfile.ZipFile(unsigned, "a", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.write(dex / "classes.dex", "classes.dex")
        for name, data in sorted(libraries.items()):
            archive.writestr(name, data)
    aligned = app / "aligned.apk"
    run(tools / "zipalign", "-f", "4", unsigned, aligned)
    keystore = app / "debug.keystore"
    if not keystore.exists():
        run(
            "keytool", "-genkeypair", "-keystore", keystore, "-storepass", "android",
            "-keypass", "android", "-alias", "androiddebugkey", "-dname", "CN=Dream validation",
            "-keyalg", "RSA", "-keysize", "2048", "-validity", "3650",
        )
    apk = app / "dream-validation.apk"
    run(
        tools / "apksigner", "sign", "--ks", keystore, "--ks-pass", "pass:android",
        "--out", apk, aligned,
    )
    run(tools / "apksigner", "verify", apk)
    return apk


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ndk", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, default=os.environ.get("ANDROID_HOME"))
    args = parser.parse_args()
    if not args.sdk:
        parser.error("--sdk or ANDROID_HOME is required")
    if os.name == "nt":
        parser.error("run this SDK validation on Linux or macOS (or the GitHub workflow)")
    output, project, dream, dreamer = prepare("android")
    host = "darwin-x86_64" if os.uname().sysname == "Darwin" else "linux-x86_64"
    toolchain = args.ndk.resolve() / "toolchains/llvm/prebuilt" / host
    slices = []
    for triple in ["aarch64-linux-android", "x86_64-linux-android"]:
        env = os.environ.copy()
        linker = toolchain / f"bin/{triple}23-clang"
        env[f"CARGO_TARGET_{triple.upper().replace('-', '_')}_LINKER"] = str(linker)
        env["DREAM_CC"] = str(toolchain / "bin/clang")
        env["DREAM_CXX"] = str(toolchain / "bin/clang++")
        env["DREAM_SYSROOT"] = str(toolchain / "sysroot")
        archive, core, env = build_slice(output, project, dream, triple, triple, env, "so")
        library = archive.with_suffix(".so")
        run(
            linker, "-shared", "-Wl,--no-undefined", "-Wl,-z,max-page-size=16384",
            "-Wl,-soname,libmobile_demo.so", "-Wl,--whole-archive", archive,
            "-Wl,--no-whole-archive", core, "-ldl", "-lm", "-llog", "-o", library,
        )
        slices += ["--slice", f"{triple}={library}"]
    run(
        dreamer, "pack", "--target", "android", "--ndk", args.ndk.resolve(),
        "--android-api", "23", "--android-package", PACKAGE, *slices,
        cwd=project, env=env,
    )
    aar = project / "target/pack/mobile_demo.aar"
    apk = build_apk(output, aar, args.sdk.resolve())
    adb = args.sdk / "platform-tools/adb"
    run(adb, "wait-for-device", timeout=120)
    run(adb, "install", "-r", apk)
    run(adb, "logcat", "-c")
    run(adb, "shell", "am", "start", "-W", "-n", f"{PACKAGE}/.MainActivity")
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        log = capture(adb, "logcat", "-d", "-s", "DreamValidation:I", "AndroidRuntime:E", "*:S")
        (output / "logcat.txt").write_text(log + "\n")
        if PASS in log:
            result("android", {"abi": capture(adb, "shell", "getprop", "ro.product.cpu.abi"),
                               "apk": str(apk), "aar": str(aar)})
            return
        if "FATAL EXCEPTION" in log:
            raise RuntimeError(log)
        time.sleep(1)
    raise RuntimeError("Android app did not report a successful Dream call; see logcat.txt")


if __name__ == "__main__":
    main()
