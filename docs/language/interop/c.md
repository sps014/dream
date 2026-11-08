---
title: C
summary: Call C and C++ sources and system libraries from Dream.
icon: material/language-c
status: stable
targets: [native, web, node]
---

# C and C++ Interop

Dream calls C and C++ on native and wasm32 builds, including Dream Task workers. Put portable sources in a package's `native/` directory, declare each native function in Dream, and let callers `import` the package.

!!! glance "At a glance"

    ```dream
    @c("m", "cbrt")
    extern fun cube_root(x: double): double;
    ```

    - Declare C functions with `@c` and call them like Dream functions.
    - Package C sources in `native/`; they build for native and WebAssembly targets.

- **C** is bound with `@c` externs.
- **C++** is bound with `@cpp`. Each declaration is checked against the real header.
- The compiler does not read headers. Declarations are ordinary Dream source, so completion, hover, and go-to-definition work on them.
- Native builds can link package sources or host libraries. A WASM build compiles portable package sources into the guest module, including WASI libc and libm (`@c("c", ...)` / `@c("m", ...)`).
- A host library such as system libsqlite3 cannot be linked into WASM. A binding the program never calls is not checked for the target.


Package C and C++ are compiled with Dream's clang, and a native program links with that same clang. C++ uses the target's standard library: libc++ on Apple platforms, libstdc++ from the system or `DREAM_SYSROOT` on Linux, and the MSVC library on Windows.

Runnable samples: [`sample/native_c/`](https://github.com/sps014/dream/tree/main/sample/native_c) and
[`sample/native_cpp/`](https://github.com/sps014/dream/tree/main/sample/native_cpp).

## Explore this topic

- [Callbacks from native libraries](native-callbacks.md)
- [Connect to a C++ library](cpp.md)

## Package layout

```
ticker/
├── dream.toml            # [package] name = "ticker"
├── src/ticker.dream      # the thin wrapper
└── native/
    ├── include/ticker.h  # on the include path
    ├── ticker.c
    └── vendor/lz4.c      # mixed C and C++ is fine
```

Everything under `native/` compiles as one native set named after the package (`kv-store` becomes `kv_store`). `native/include/` is on the include path.

`.c` files compile as `gnu11`, and `.cpp`, `.cc`, and `.cxx` files as `gnu++20`. The C++ standard library is linked only when the set contains C++.

A set is built only when the program calls into it. Native objects live in `target/<profile>/native-c/<set>/` and rebuild when a source is newer, and WASM output for the set is rebuilt in the module's `.wasm-c/` directory.

### `dream.toml`

A `[native.<set>]` table is optional. It adds build settings to the implicit set, or declares an
extra set with explicit `sources`:

```toml
[package]
name = "kvstore"
links = "kv"                  # optional: at most one package in the graph may provide "kv"

[native.kvstore]
cflags  = ["-fno-exceptions"]
defines = ["KV_THREADSAFE=1"]

[native.kvstore.macos]
frameworks = ["Security"]

[native.kvstore.linux]
libs = ["dl"]
```

| Key | Meaning |
|-----|---------|
| `sources` | Extra source files, relative to the package root |
| `include` | Extra include directories |
| `defines` | `-D` definitions |
| `cflags` | Extra compiler flags |
| `frameworks` | macOS frameworks to link |
| `libs` | System libraries to link (`-l`) |

Target subtables (`macos`, `linux`, `windows`, `wasm`) take the same keys and apply only to that target. A WASM build does not inherit the host OS table.

A set with no sources for the selected target, or a missing source file, is an error. An unknown key, two packages with the same set name, or two packages with the same `[package] links` value is also an error, and the message names the manifests.

## WebAssembly

The same `native/` directory and `@c` / `@cpp` declarations work for portable WASM packages. Add guest-only flags with `[native.<set>.wasm]`.

Use `usize` and `isize` for C `size_t` and `ptrdiff_t`. Dream `long` is always 64 bits, and a scalar signature that does not match the compiled function is a build error.

Portable package sources become part of the WebAssembly program. A prebuilt desktop library cannot be used this way.

C++ uses the WASI SDK 33 libc++ with exceptions, plus libc++abi and libunwind. A thrown exception still becomes a `Result` when the Dream declaration says so.

Global constructors run after Dream's heap is ready. Exit hooks run registered destructors and flush stdio after `main`, and the JS loader prints constructor output in both runtimes.

C allocation uses the same memory as Dream. Pointers and callbacks stay inside that module, and a stored callback must be called and released on the instance that created it.

Owned pointers, UTF-8 strings, array data, unmanaged structs, and callbacks use the same declarations as a native build.

Supported libc covers stdout, stderr, clocks, and an empty process environment. An OS-specific function or a missing library symbol fails the build, so use `@js` for browser and Node services.

In a `Task` module, a worker pool keeps C thread-local values between dispatches. Global constructors run once after the primary instance is ready.

Workers come from Dream's task runtime, so WASI `pthread_create` is not available. Use `@native`, `@node`, or `@web` to limit a binding to one runtime.

Development installs need the full WASI sysroot next to the installed build tools; update it with
`scripts/fetch-dev-llvm.sh` or `scripts/fetch-dev-llvm.ps1` on Windows. Release packaging includes
clang, its resource headers, and the wasm32 WASI headers/libraries needed by package builds.

## C: `@c`

```dream
import system.interop;

module ticker;

import system;

@c
extern fun ticker_new(name: string): CPtr;

@c
extern fun ticker_run(t: CPtr, ticks: int): int;

@c("m", "cbrt")
extern fun cube_root(x: double): double;
```

- **Bare `@c`** binds to the declaring package's own `native/` set. The Dream name is the C symbol.
- **`@c("lib")`** names another library, and **`@c("lib", "symbol")`** also renames the symbol. A
  library that is not a native set is linked from the system (`-l<lib>`); see
  [Linking system libraries](#linking-system-libraries).
- `@c` cannot be combined with `@js`, `@runtime`, or `@intrinsic`.

### Types at the boundary

| Dream | C |
|-------|---|
| `int`, `long`, `float`, `double` | `int32_t`, `int64_t`, `float`, `double` |
| `isize`, `usize` | `intptr_t`, `uintptr_t` (`usize` also matches `size_t`) |
| `bool`, `char`, `byte` | `bool`, `char`, `uint8_t` |
| `string` parameter | `const char*`: NUL-terminated UTF-8, valid for the call |
| `string` result | `const char*`, copied into a Dream `string`; C keeps ownership |
| `CPtr` | `void*` / `T*` |
| `Option<string>`, `Option<CPtr>` | a pointer where `NULL` is `None` |
| `T[]` (numbers, `byte`, plain-data structs) | `T*` to the first element, valid for the call |
| plain-data value struct (see [Structs](cpp.md#structs)) | the C struct by value, or `T*` as `ref x: T` |
| `ref x: T` | `T*`: C writes through it (`ref p: CPtr` is `T**`) |
| `fun(...)` | a C function pointer (see [Callbacks](cpp.md#callbacks)) |
| `NativeCallback<F>` | a `(fn, void* user_data)` pair |

By-value `char` converts to C’s narrow `char`, including fields of structs passed by value.
Pointers and `ref` arguments expose Dream storage: a `char` occupies four bytes, so use
`int32_t*` for a character slot and `byte[]` for a raw C character buffer.
| `Option<fun(...)>`, `Option<NativeCallback<F>>` | a nullable function pointer |
| `OwnedCPtr` result with `@owned("free_fn")` | `void*` that Dream frees with `free_fn` |

Classes, unions, and arrays of managed elements are rejected in `@c` signatures. Wrap them in a Dream function that passes C only what it needs.

A `NULL` returned where the declaration says `string` traps with a message naming the function.
Declare the result as `Option<string>` when C may return `NULL`.

Arrays pass only the pointer. When C needs the length, declare it as a separate parameter and pass
`xs.length`:

```dream
import system.interop;

@c
extern fun checksum(data: byte[], len: long): int;

public fun fingerprint(data: byte[]): int {
    return checksum(data, data.length);
}
```

### `CPtr`

`CPtr` (in `system`) is an opaque pointer: `CPtr.null()`, `is_null()`, `==`, `address(): usize`, and `offset(bytes: isize)`, with a target-sized address. Use it for handles, and `ref` for C out-parameters:

```dream
import system.interop;

@c("sqlite3")
extern fun sqlite3_open(path: string, ref db: CPtr): int;

let db = CPtr.null();
if sqlite3_open("app.db", ref db) != 0 {
    ...
}
```

`Ffi.read_ptr`, `Ffi.read_int`, `Ffi.read_long`, `Ffi.read_double`, and `Ffi.read_cstring` read
through pointers C hands to Dream (`char**` rows and similar).

### Owned pointers

When C returns a pointer the caller must free, return `OwnedCPtr` and name the free function with `@owned("free_fn")`. Dream calls `free_fn(ptr)` when your program no longer uses the object.

`get()` reads the pointer while Dream still owns it. `take()` gives ownership back and leaves the object holding `NULL`.

```dream
import system.interop;

@c
@owned("ticker_free")
extern fun ticker_new(name: string): OwnedCPtr;

@c
extern fun ticker_run(t: CPtr, ticks: int): int;

let t = ticker_new("demo");
ticker_run(t.get(), 3); // ticker_free runs when `t` goes out of scope
```

`free_fn` must be a C identifier with the signature `void free_fn(void*)`. A `@c` extern returning
`OwnedCPtr` without `@owned("free_fn")`, or `@owned("free_fn")` on any other result, is an error.

### Structs

A value struct of numbers, `bool`, `char`, `byte`, or other such structs matches a C struct with the same fields in the same order. Pass or return it by value, or pass `ref` so C sees `T*`.

Mark it `@packed` when the C header uses `#pragma pack(1)` or `__attribute__((packed))`. Otherwise fields are naturally aligned, as in C.

```dream
import system.interop;

public struct Vec2 {
    public x: double;
    public y: double;
}

@c
extern fun vec2_scale(v: Vec2, k: double): Vec2; // C: Vec2 vec2_scale(Vec2 v, double k)

@c
extern fun vec2_len(ref v: Vec2): double; // C: double vec2_len(const Vec2* v)

@c
extern fun vec2_origin(ref out: Vec2): void;
```

### Strings

`string` parameters are copied into a NUL-terminated UTF-8 buffer that is freed after the call.
`@marshal(StringEncoding.Utf16)` passes UTF-16 instead (`char16_t*` / Windows `LPWSTR`, not `wchar_t`). The same encoding applies to the extern's string result and to string parameters of its callbacks:

```dream
import system.interop;

@c("user32", "MessageBoxW")
@marshal(StringEncoding.Utf16)
extern fun MessageBoxW(hwnd: CPtr, text: string, caption: string, flags: int): int;
```

Externs use the platform C calling convention.

## Packages

- Packages under `dream_packages/` bring their own `native/` sets. A consumer only imports the
  package; its sets are built and linked when the program calls into them.
- Shim symbols are namespaced by native set, so two packages can each bind a C++ class called
  `Store`. Dream class names are program-wide, so give the Dream classes distinct names
  (`@cpp("alpha.hpp", "alpha::Store") public class AlphaStore`).
- `[package] links = "sqlite3"` declares that a package provides a native library, and at most one
  package in the dependency graph may do so.
- `dreamer pack` produces a single executable. Native sets are linked in statically.

## Linking system libraries

On native targets, `@c("lib", ...)` with a library that is not a native set links `-l<lib>`. Search order:

1. `native/<lib>` next to the source (vendored shared libraries).
2. The directory containing the source, then the current working directory.
3. Standard system directories:
   - macOS: `/opt/homebrew/lib`, `/usr/local/lib`, `/opt/local/lib`, `/usr/lib`
   - Linux: `/usr/local/lib`, `/usr/lib/x86_64-linux-gnu`, `/usr/lib/aarch64-linux-gnu`,
     `/usr/lib64`, `/usr/lib`, `/lib`
   - Windows: `%WINDIR%\System32`
4. The OS loader's own path (`DYLD_FALLBACK_LIBRARY_PATH` / `LD_LIBRARY_PATH`).

[`sample/sqlite/`](https://github.com/sps014/dream/tree/main/sample/sqlite) binds the system libsqlite3 this way:

```bash
dream run sample/sqlite/db.dream
```

## Compared to `@js` and `@runtime`

| Concern | `@js("mod", "field")` | `@runtime("name")` | `@c` / `@cpp` |
|---------|-----------------------|--------------------|---------------|
| Host | JS (Node / browser) | Dream runtime (WASM + native) | Native and portable wasm32 package builds |
| Async | `async` + `Promise` bridge | `async` + host future | Not supported; C runs on the caller's thread |
| Out-params | Return a wrapper struct / tuple | Return a wrapper struct / tuple | `ref` parameters |
| Callbacks into Dream | `fun(...)` values | host-defined | `fun` pointers, `NativeCallback`, `std::function` |
| Null | `Option` | `Option` | `Option` |
