---
title: Mobile libraries
summary: Package Dream libraries as an iOS XCFramework or Android AAR.
icon: material/cellphone
---

# Mobile library packages

## Canvas application projects

```sh
dreamer init mobile phone
dreamer init app counter
```

`mobile` declares Android and iOS; `app` also declares the desktop host and selects it by
default. Both generate a shared GPU canvas counter, including touch-event handling,
scale handling, and safe-area offsets. The renderer includes its default Noto Sans font.
Edit the source and `[app]` identity in `dream.toml` as described in
[Create a project](../tooling/create-project.md).

The canvas scaffold is available, but Android/iOS window, presentation, lifecycle, and
application-shell integration are not complete. `build` and `run` report that limitation
for mobile targets; they do not produce APKs or simulator apps yet. Desktop canvas builds,
browser touch delivery, and headless touch/inset simulations can be tested independently.

## Inspect the mobile toolchain

Before building mobile libraries, check the installed SDKs and discover devices:

```sh
dreamer toolchain doctor --target android
dreamer toolchain doctor --target ios
dreamer devices
```

For Android, set `ANDROID_HOME` to the installed SDK directory (`ANDROID_SDK_ROOT`
is used when `ANDROID_HOME` is absent). Install platform-tools, an SDK platform at
API 26 or newer, build-tools, an NDK, and JDK 21 or newer on `PATH`. Set
`ANDROID_NDK_HOME` to select an NDK explicitly; otherwise the doctor selects the
newest installed stable NDK. On Apple Silicon it uses the `darwin-arm64` NDK
prebuilt when that directory is present, and the `darwin-x86_64` prebuilt otherwise. These are prerequisites for the canvas app development
tooling; the library packager's minimum API remains configurable below.

For iOS, install full Xcode, select its developer directory with `xcode-select`, and
install an iOS simulator runtime through Xcode Settings > Components. Command-line
tools alone do not include the iOS SDKs. iOS development requires macOS.

The doctor checks both platform architectures and reports missing target-built
core, window, GPU, drawing, and app capability libraries. It does not install
components or qualify GPU rendering. A successful prerequisite check does not
establish that a canvas app can run: SDK-backed app validation is still required.

`dreamer devices --target android` lists connected Android devices and emulators,
including offline or unauthorized devices. Authorize USB debugging on the device
before using it. `dreamer devices --target ios` lists available iOS simulators,
including shut-down simulators, without booting them. With no target, discovery
checks both platforms and reports any unavailable platform separately.

Add `--json` to either command for structured output. A targeted discovery fails
when its SDK cannot be queried; unfiltered discovery succeeds when at least one
platform can be queried, even if no devices are present. The doctor exits with a
failure when any requirement is missing. Mobile canvas build/run remains unavailable.

## Package a library

`dreamer pack` can package target-built Dream libraries as an iOS XCFramework or an Android AAR.
The input libraries must already include the guest runtime and their target-specific host
capability dependencies. Cross-target library linking and host builds are separate steps;
`dream --object --target` emits an unlinked object without a platform SDK. `dream --target` links the library selected in dream.toml and requires the platform SDK and target capability libraries.

## Compile an interface and object

A mobile package uses `type = "lib"` in `dream.toml` and `@export` functions. For example:

```dream
@export
fun add(a: int, b: int): int {
    return a + b;
}
```

```toml
[package]
name = "demo"
version = "0.1.0"
type = "lib"

[lib]
output-type = "staticlib"
```

Use `output-type = "cdylib"` when building a host shared library. Cross emission produces
an object for either kind; the SDK linker determines the final mobile slice format.

```sh
dream --object --target arm64-apple-ios --min-os 13.0 src/lib.dream -o build/device/libdemo.ll
dream --object --target arm64-apple-ios-simulator --min-os 13.0 src/lib.dream -o build/simulator/libdemo.ll
dream --object --target aarch64-linux-android src/lib.dream -o build/arm64/libdemo.ll
dream --object --target x86_64-linux-android src/lib.dream -o build/x64/libdemo.ll
```

Create the output directories first. Each command produces `.o`, `.ll`, `.h` and `.abi.json`.
Compile the guest C runtime and the required capability libraries for the same SDK/target and
link them with this object, producing `libdemo.a` on iOS or `libdemo.so` on Android. Keep the
compiler-generated `libdemo.h` and `libdemo.abi.json` next to each final library. Their signatures
must match across slices. The packager checks target identity and the exported signature inventory.
A desktop library cannot serve as a mobile slice.

## `dreamer pack` mobile options

| Option | Effect |
|---|---|
| `--target ios` / `--target android` | Package a mobile library instead of a desktop executable |
| `--slice <triple>=<library>` | One target-built library; repeat for every required slice |
| `--ndk <path>` | Android NDK folder (falls back to `ANDROID_NDK_HOME`) |
| `--android-api <level>` | Minimum Android API level (default `21`, minimum `21`) |
| `--android-package <name>` | Java package for the bridge class (default `dream.generated.<name>`) |
| `-p <name>` / `--package <name>` | Workspace member to package |

Mobile packing requires a `type = "lib"` package whose name is a C identifier once hyphens become underscores. Compilation flags such as `--release` and `-O` are rejected, because the libraries are already built.

| Platform | Required slices | Library file | Result |
|---|---|---|---|
| `ios` | `arm64-apple-ios`, `arm64-apple-ios-simulator` | `.a` | `target/pack/<name>.xcframework` |
| `android` | `aarch64-linux-android`, `x86_64-linux-android` | `.so` | `target/pack/<name>.aar` |

Each library's file name (without extension) must be a C identifier, and its `.h` and `.abi.json` must sit next to it.

## iOS

Full Xcode, with the iPhoneOS and iPhoneSimulator SDKs, is required. The package contains
arm64 device and arm64 simulator slices; iOS deployment defaults to 13.0 and follows the
slice's ABI target metadata.

```sh
dreamer pack --target ios \
  --slice arm64-apple-ios=build/device/libdemo.a \
  --slice arm64-apple-ios-simulator=build/simulator/libdemo.a
```

The result is `target/pack/demo.xcframework` (using the manifest package name).
Each slice includes the C header, a module map and the `Dream_demo` Objective-C class.
The packager compiles its generated Objective-C bridge for that slice, archives it with the
input library and invokes `xcodebuild -create-xcframework`. Link Foundation in the host app.
Methods are named `call_<export>`, with `arg1:`, `arg2:`, etc. for additional arguments.

```objc
[Dream_demo attach];
int32_t value = [Dream_demo call_add:20 arg1:22];
[Dream_demo detach];
```

See Apple's [multiplatform binary framework guide](https://developer.apple.com/documentation/xcode/creating-a-multi-platform-binary-framework-bundle).

## Android

An Android NDK and a JDK 9+ are required. Pass the NDK with `--ndk` or set `ANDROID_NDK_HOME`.
The minimum API defaults to 21 and must be at least 21 for these 64-bit ABIs. Input libraries
must also support this minimum API and Android's page-size requirements.

```sh
dreamer pack --target android --ndk /path/to/android-ndk --android-api 21 \
  --android-package org.example.demo \
  --slice aarch64-linux-android=build/arm64/libdemo.so \
  --slice x86_64-linux-android=build/x64/libdemo.so
```

The result is `target/pack/demo.aar`, with `jni/arm64-v8a/` and `jni/x86_64/`, a compiled
`classes.jar`, `AndroidManifest.xml`, consumer keep rules and bridge sources. The Java class
is `org.example.demo.DreamLibrary`; it loads `demo_jni`. Its native methods are also callable
from Kotlin. The JNI shared libraries link against the supplied library's public C exports;
their links reject unresolved symbols and use a 16 KiB maximum page size.

```java
DreamLibrary.attach();
try {
    int value = DreamLibrary.call_add(20, 22);
} finally {
    DreamLibrary.detach();
}
```

See the NDK's [other build systems guide](https://developer.android.com/ndk/guides/other_build_systems)
and [Android library archive structure](https://developer.android.com/studio/projects/android-library#aar-contents).

## SDK-equipped sample-app validation

iOS/Android end-to-end validation is **on hold for a future version** (2026-10-04).
The sample apps and manual workflow are retained; simulator/emulator execution has not
completed, so mobile release readiness is unverified.

The manual **Mobile sample apps** GitHub Actions workflow is prepared to build the real Rust core and
Dream static library for both iOS slices and both Android ABIs. It packages an XCFramework
and an AAR, links the library into the sample apps in `sample/mobile/`, and executes an
iPhone simulator app and an Android x86_64 emulator app. Each app attaches a thread, calls
the generated bridge, checks the array-based Dream function returns 42, and detaches.
The workflow preserves packages, app binaries, runtime evidence and `result.json` reports.
Device iOS and Android arm64 slices are built; execution is on simulators/emulators.

To repeat the checks on an SDK-equipped machine, first install the pinned LLVM and build
the compiler and packager:

```sh
scripts/fetch-dev-llvm.sh
cargo build --locked -p dream --no-default-features --features native -p dreamer
python3 scripts/mobile/ios.py # Apple Silicon Mac with Xcode and an iOS simulator
python3 scripts/mobile/android.py --ndk "$ANDROID_HOME/ndk/28.2.13676358"
```

Android validation requires a running emulator/device, JDK 21, SDK platform/build-tools 34,
platform-tools and the NDK; it runs on Linux or macOS. The sample APK uses a generated local
debug signing key. Both checks write outputs under `target/mobile-validation/`, and fail
if the app never reports the expected result. Cross-target core capability binaries are
built from the repository rather than substituted with test stubs.

## Ownership and threading

The bridges forward the generated plain C ABI without a hidden caller location. Scalar types
map to equivalent Objective-C types and Java primitives. Java `int`/`long` preserve the bits
of unsigned Dream integers. Managed objects, strings and boxed values stay opaque pointers:
Objective-C uses `void *`, and Java/Kotlin use `long` handles. There is no automatic conversion
to NSString, Java String, or managed host objects.

A taken parameter consumes one Dream reference; a borrowed/ref parameter does not. An owning
result must eventually be released with `releaseHandle`. Handle releases and calls belong on
an attached thread. Keep a thread attached while its callbacks are live; detach before that
thread exits. Serialize the first exported call, which initializes module globals. Panics abort
rather than unwind through Objective-C or JNI. Package-relative panic paths survive packaging.
