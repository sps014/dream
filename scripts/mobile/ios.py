"""Package both iOS slices and run the Objective-C bridge in a simulator app."""

import json
import os
from pathlib import Path
import plistlib
import time

from common import PACKAGE, PASS, SAMPLE, build_slice, capture, prepare, result, run


def main():
    if os.uname().sysname != "Darwin" or os.uname().machine != "arm64":
        raise RuntimeError("iOS validation requires Apple Silicon macOS with full Xcode")
    output, project, dream, dreamer = prepare("ios")
    slices = []
    for rust_target, dream_target in [
        ("aarch64-apple-ios", "aarch64-apple-ios"),
        ("aarch64-apple-ios-sim", "aarch64-apple-ios-sim"),
    ]:
        env = os.environ.copy()
        env["IPHONEOS_DEPLOYMENT_TARGET"] = "13.0"
        llvm = os.environ.get("DREAM_LLVM", os.path.expanduser("~/.dream/toolchains/llvm-22.1.8/bin"))
        env["DREAM_CC"] = llvm + "/clang"
        env["DREAM_CXX"] = llvm + "/clang"
        archive, core, env = build_slice(
            output, project, dream, rust_target, dream_target, env, "dylib"
        )
        selected = "arm64-apple-ios-simulator" if dream_target.endswith("-sim") else "arm64-apple-ios"
        slices += ["--slice", f"{selected}={archive}"]
    run(dreamer, "pack", "--target", "ios", *slices, cwd=project, env=env)
    framework = project / "target/pack/mobile_demo.xcframework"
    info = plistlib.loads((framework / "Info.plist").read_bytes())
    simulator_slice = next(
        item for item in info["AvailableLibraries"]
        if item.get("SupportedPlatformVariant") == "simulator"
    )
    directory = framework / simulator_slice["LibraryIdentifier"]
    headers = directory / simulator_slice["HeadersPath"]
    library = directory / simulator_slice["LibraryPath"]
    app = output / "app/DreamValidation.app"
    app.mkdir(parents=True, exist_ok=True)
    (app / "Info.plist").write_bytes(plistlib.dumps({
        "CFBundleIdentifier": PACKAGE,
        "CFBundleExecutable": "DreamValidation",
        "CFBundleName": "DreamValidation",
        "CFBundlePackageType": "APPL",
        "CFBundleVersion": "1",
        "CFBundleShortVersionString": "1.0",
        "MinimumOSVersion": "13.0",
        "LSRequiresIPhoneOS": True,
        "UIDeviceFamily": [1, 2],
        "UILaunchScreen": {},
    }))
    sdk = capture("xcrun", "--sdk", "iphonesimulator", "--show-sdk-path")
    run(
        "xcrun", "--sdk", "iphonesimulator", "clang", "-target", "arm64-apple-ios13.0-simulator",
        "-isysroot", sdk, "-fobjc-arc", SAMPLE / "ios/main.m", "-I", headers,
        library, core, "-framework", "UIKit", "-framework", "Foundation",
        "-framework", "Security", "-framework", "CoreFoundation", "-liconv", "-lresolv",
        "-o", app / "DreamValidation",
    )
    run("codesign", "--force", "--sign", "-", app)
    devices = json.loads(capture("xcrun", "simctl", "list", "devices", "available", "--json"))
    device = next(
        device for runtime, group in devices["devices"].items()
        if ".iOS-" in runtime for device in group
        if device.get("isAvailable") and device["name"].startswith("iPhone")
    )
    udid = device["udid"]
    owned_boot = device["state"] != "Booted"
    try:
        if owned_boot:
            run("xcrun", "simctl", "boot", udid)
        run("xcrun", "simctl", "bootstatus", udid, "-b", timeout=300)
        run("xcrun", "simctl", "install", udid, app)
        container = capture("xcrun", "simctl", "get_app_container", udid, PACKAGE, "data")
        evidence = Path(container) / "Documents/dream-result.txt"
        evidence.unlink(missing_ok=True)
        run("xcrun", "simctl", "launch", "--terminate-running-process", udid, PACKAGE)
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if evidence.is_file() and evidence.read_text().strip() == PASS:
                result("ios", {"device": device["name"], "udid": udid,
                               "xcframework": str(framework), "app": str(app)})
                return
            time.sleep(1)
        raise RuntimeError("iOS app did not report a successful Dream call")
    finally:
        if owned_boot:
            run("xcrun", "simctl", "shutdown", udid)


if __name__ == "__main__":
    main()
