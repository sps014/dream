---
title: Mobile apps and libraries
summary: Build and run a Dream canvas app on Android or iOS, or package a library as an AAR or XCFramework.
icon: material/cellphone
---

# Mobile apps and libraries

## App

```sh
dreamer init mobile phone
dreamer devices
dreamer run --target android
dreamer run --target ios
```

`mobile` is Android and iOS. `dreamer init app` adds the desktop host and uses it when you omit `--target`. Both start from the same canvas counter.

`dreamer build --target android` writes a debug APK. `dreamer build --target ios` writes a simulator app. Output is under `target/<triple>/debug/`, or `release/` with `--release`. `dreamer run` installs that build, launches it, and follows its log until you press Ctrl+C.

`--device` picks an id from `dreamer devices`. Without it, `run` uses the only running device. Android follows that device's architecture; a build with no device is `arm64-v8a`. iOS builds an arm64 simulator app on Apple silicon. Store signing and install on a physical iPhone are not supported yet.

Put files the app should read in `[package].assets`. The app starts in the folder that contains them, so open them with `File` and the same relative path:

```toml
[package]
assets = ["config.json", "images"]
```

```dream
import system.io;

async fun main(): void {
    let config = (File.read("config.json").await).unwrap_or("");
    let image = match File.read_bytes("images/hero.png").await {
        Ok(bytes) => bytes,
        Err(_) => return,
    };
}
```

On Android that folder is the app's private storage, so `File.write` can save files there too. On iOS it is the app bundle.

`main` returns after startup. Timers, input, and drawing continue on the platform's frame loop. A hidden or replaced Android surface pauses drawing until a surface is available again.

Check the SDK before the first build:

```sh
dreamer toolchain doctor --target android
dreamer toolchain doctor --target ios
```

Android needs `ANDROID_HOME`, platform-tools, an SDK platform (API 35 or newer), build-tools, an NDK, and JDK 21 or newer. Apps run on Android 8.0 and newer. iOS needs full Xcode and a simulator runtime, on a Mac. `doctor` also reports missing Dream libraries for that phone; `build` and `run` stop until those are present.

## Library

`dreamer pack --target ios` or `--target android` packages a `type = "lib"` project that is already built for each phone slice. The libraries must contain the guest runtime and their host dependencies. `dream --object --target <triple>` emits the unlinked object, header, and `.abi.json`; you compile and link that object with the platform SDK.

```sh
dreamer pack --target ios \
  --slice arm64-apple-ios=build/device/libdemo.a \
  --slice arm64-apple-ios-simulator=build/simulator/libdemo.a

dreamer pack --target android \
  --slice aarch64-linux-android=build/arm64/libdemo.so \
  --slice x86_64-linux-android=build/x64/libdemo.so
```

| Platform | Slices | Result |
|---|---|---|
| iOS | `arm64-apple-ios`, `arm64-apple-ios-simulator` (`.a`) | `target/pack/<name>.xcframework` |
| Android | `aarch64-linux-android`, `x86_64-linux-android` (`.so`) | `target/pack/<name>.aar` |

Each library needs its `.h` and `.abi.json` beside it, and the exported signatures must match across slices. `--ndk` or `ANDROID_NDK_HOME` selects the NDK. `--android-api` defaults to 21. `--android-package` sets the Java package (`dream.generated.<name>` by default).

Call an export on an attached thread, then detach. A borrowed argument does not consume a Dream reference; an owned result is released with `releaseHandle`.

```objc
[Dream_demo attach];
int32_t value = [Dream_demo call_add:20 arg1:22];
[Dream_demo detach];
```

```java
DreamLibrary.attach();
try {
    int value = DreamLibrary.call_add(20, 22);
} finally {
    DreamLibrary.detach();
}
```

Strings and objects cross the bridge as opaque handles (`void *` in Objective-C, `long` in Java), not as `NSString` or `String`. Link Foundation in the iOS host app.
