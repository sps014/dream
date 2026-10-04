# Mobile library packages

`dreamer pack` can package target-built Dream libraries as an iOS XCFramework or an Android AAR.
The input libraries must already include the guest runtime and their target-specific host
capability dependencies. Cross-target library linking and host builds are separate steps;
`dream --object --target` emits an unlinked object without a platform SDK. `dream --target` links the library selected in dream.toml and requires the platform SDK and target capability libraries.

## Compile an interface and object

A mobile package uses `type = "lib"` in `dream.toml` and `@export` functions. For example:

```dream
@export
fun add(a: int, b: int): int { return a + b; }
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
