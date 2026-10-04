# C and C++ Interop

Dream calls C and C++ directly on native builds. A package puts portable C or C++ sources in `native/`,
then writes a thin Dream wrapper: one declaration per native function or method, plus whatever
idiomatic Dream API it wants on top. Consumers just `import` the package.

- **C** is bound with `@c` externs.
- **C++** is bound with `@cpp` declarations. The compiler generates a small C++ shim that implements
  them, and the C++ compiler checks each declaration against the real header.
- The compiler never parses headers. Declarations are ordinary Dream source, so completion, hover,
  and go-to-definition work on them.
- **Native-only.** A wasm32 build that still calls a `@c`/`@cpp` declaration is a compile error that
  names it. Bind the browser/Node equivalent with [`@js`](interop.md) or guard the call with `@native`.
  Unused declarations and calls in unreachable functions are pruned before this check, so a
  shared package can contain native bindings without forcing a wasm32 consumer to link them.

Zig (installed by `dreamer toolchain install cc`) compiles the C/C++ and links it with the Dream
object, so no other toolchain is needed. `DREAM_CXX` / `CXX` override the C++ compiler.

Runnable samples: [`sample/native_c/`](../../../sample/native_c) and
[`sample/native_cpp/`](../../../sample/native_cpp).

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

Everything under `native/` compiles as one **native set** named after the package (`kv-store` →
`kv_store`). `native/include/` is on the include path. `.c` files compile as `gnu11`, and `.cpp` /
`.cc` / `.cxx` files as `gnu++20`. libc++ is linked only when the set contains C++.

A set is built only when the program calls into it, into `target/<profile>/native-c/<set>/`. Objects
are rebuilt only when a source is newer.

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

Per-OS subtables (`macos`, `linux`, `windows`) take the same keys and are appended on that host.
A set with no sources on the current host, or a source that does not exist, is an error. So are an
unknown key, two packages declaring the same set name, and two packages declaring the same
`[package] links` value. Each of these errors names the manifests involved.

## C: `@c`

```dream
module ticker;

import system;

@c extern fun ticker_new(name: string): CPtr;
@c extern fun ticker_run(t: CPtr, ticks: int): int;
@c("m", "cbrt") extern fun cube_root(x: double): double;
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
| plain-data value struct (see [Structs](#structs)) | the C struct by value, or `T*` as `ref x: T` |
| `ref x: T` | `T*`: C writes through it (`ref p: CPtr` is `T**`) |
| `fun(...)` | a C function pointer (see [Callbacks](#callbacks)) |
| `NativeCallback<F>` | a `(fn, void* user_data)` pair |
| `Option<fun(...)>`, `Option<NativeCallback<F>>` | a nullable function pointer |
| `OwnedCPtr` result with `@owned("free_fn")` | `void*` that Dream frees with `free_fn` |

Every `@c` call goes through a small C shim the compiler generates and compiles with the pinned
clang, so the platform C ABI (struct passing and return, `bool`/`char` widths, calling
conventions) is clang's, not a hand-written copy.

Classes, unions, and arrays of managed elements are rejected in `@c` signatures. They are heap
references that C does not understand. Wrap them in a Dream function that passes C what it needs.

A `NULL` returned where the declaration says `string` traps with a message naming the function.
Declare the result as `Option<string>` when C may return `NULL`.

Arrays pass only the pointer. When C needs the length, declare it as a separate parameter and pass
`xs.length`:

```dream
@c extern fun checksum(data: byte[], len: long): int;

public fun fingerprint(data: byte[]): int {
    return checksum(data, data.length);
}
```

### `CPtr`

`CPtr` (in `system`) is an opaque C pointer. It has `CPtr.null()`, `is_null()`, `==`,
`address(): usize`, and `offset(bytes: isize)`. Its single address field is target-sized. Use it for handles, and use `ref` for C's out-parameters:

```dream
@c("sqlite3") extern fun sqlite3_open(path: string, ref db: CPtr): int;

let db = CPtr.null();
if sqlite3_open("app.db", ref db) != 0 { ... }
```

`Ffi.read_ptr`, `Ffi.read_int`, `Ffi.read_long`, `Ffi.read_double`, and `Ffi.read_cstring` read
through pointers C hands to Dream (`char**` rows and similar).

### Owned pointers

When C hands back a pointer the caller must free, return `OwnedCPtr` and name the C function that
frees it with `@owned("free_fn")`. The object calls `free_fn(ptr)` when its last reference goes
away. `get()` reads the pointer while the object keeps owning it; `take()` hands ownership back to
you and leaves the object holding `NULL`.

```dream
@c @owned("ticker_free") extern fun ticker_new(name: string): OwnedCPtr;
@c extern fun ticker_run(t: CPtr, ticks: int): int;

let t = ticker_new("demo");
ticker_run(t.get(), 3); // ticker_free runs when `t` goes out of scope
```

`free_fn` must be a C identifier with the signature `void free_fn(void*)`. A `@c` extern returning
`OwnedCPtr` without `@owned("free_fn")`, or `@owned("free_fn")` on any other result, is an error.

### Structs

A value struct whose fields are numbers, `bool`/`char`/`byte`, or other such structs maps to the C
struct with the same fields in the same order. Pass it by value, return it by value, or pass it by
`ref`, which C sees as `T*`. Mark structs `@packed` when the C header uses `#pragma pack(1)` /
`__attribute__((packed))`. Otherwise fields are naturally aligned, as in C.

```dream
public struct Vec2 {
    public x: double;
    public y: double;
}

@c extern fun vec2_scale(v: Vec2, k: double): Vec2; // C: Vec2 vec2_scale(Vec2 v, double k)
@c extern fun vec2_len(ref v: Vec2): double;        // C: double vec2_len(const Vec2* v)
@c extern fun vec2_origin(ref out: Vec2): void;
```

### Strings

`string` parameters are copied into a NUL-terminated UTF-8 buffer that is freed after the call.
`@marshal("lpwstr")` passes UTF-16 instead (Windows `LPWSTR`):

```dream
@c("user32", "MessageBoxW")
@marshal("lpwstr")
extern fun MessageBoxW(hwnd: CPtr, text: string, caption: string, flags: int): int;
```

Externs use the platform C calling convention. `@c_call("cdecl")` spells that out explicitly.
`@c_call("stdcall")` selects `__stdcall` on 32-bit x86 Windows and is the default convention
everywhere else. Any other convention is a compile error.

## Callbacks

There are two ways to hand C a function.

**A plain `fun`** becomes a C function pointer when it is statically known: a named function or a
non-capturing lambda written at the call. When the C signature needs conversion (a `const char*`
parameter arriving as `string`, or a pointer arriving as `CPtr`), the compiler emits a small C-ABI
wrapper for that function. Passing a `fun` held in a variable where conversion is needed is an
error that points at `NativeCallback`.

```dream
@c("c") extern fun qsort(base: int[], n: usize, size: usize, cmp: fun(CPtr, CPtr): int): void;

qsort(xs, (usize)5, (usize)sizeof(int), (a: CPtr, b: CPtr) => Ffi.read_int(a, 0) - Ffi.read_int(b, 0));
```

**`NativeCallback<F>`** (in `system`) wraps any `fun`, capturing or not. Use it for C APIs that
take a function pointer plus a `void* user_data`. The parameter expands to both C arguments. By
default the callback receives `user_data` as its first C parameter. Mark the parameter
`@marshal("user_data_last")` for APIs that pass it last.

```c
typedef int (*ticker_fn)(void* user, int tick);
void ticker_on_tick(ticker* t, ticker_fn fn, void* user);
```

```dream
@c extern fun ticker_on_tick(t: CPtr, cb: NativeCallback<fun(int): int>): void;

public class Ticker {
    handle: CPtr;
    listener: Option<NativeCallback<fun(int): int>>;

    public fun on_tick(f: fun(int): int): void {
        let cb = NativeCallback<fun(int): int>(f);
        ticker_on_tick(this.handle, cb);
        this.listener = Option.Some(cb); // C keeps calling it: keep it alive
    }
}
```

The callback's lifetime is ordinary ARC. Keep the `NativeCallback` in a field for as long as C may
call it. Pass a temporary when C only calls it during the call (`qsort_r`, `sqlite3_exec`).

Each `NativeCallback` belongs to the Dream thread that constructed it. Calls and retains must
run on that owner, even when another thread is a Dream `Task` worker; other threads trap with an
owner-thread diagnostic.

A plain `fun` passed as a C function pointer has no owner. A thread C started itself may call it
after `dream_thread_attach()`, and must call `dream_thread_detach()` before it exits. Calling
without attaching traps with a message that names `dream_thread_attach`. Both functions are
declared in `dream_embed.h`, which every `native/` source can include:

```c
#include <dream_embed.h>

static void* worker(void* arg) {
    struct job* j = arg;
    dream_thread_attach();
    j->result = j->fn(j->input); // a Dream `fun` passed to C
    dream_thread_detach();
    return 0;
}
```

`dream_embed.h` also declares `dream_retain`/`dream_release` (for C that stores a Dream reference
past a call) and `dream_set_panic_hook` (see [Panics](panics.md#embedding-a-panic-hook)).

C++ adapters may release their retained callback from any thread. Such releases are queued
without touching the object's non-atomic reference count; the owner drains them at scheduler
ticks, worker wakeups, or shutdown. Destructors run on the owner. Join foreign users and release
their callback handles before the owner exits; a callback that outlives its owner is an error.

## C++: `@cpp`

Declare the C++ API you need in Dream. The header stays ordinary C++, with no annotations and no
`extern "C"`:

```cpp
// native/include/kv.hpp
namespace kv {
struct Entry { std::string key; const std::string& name() const { return key; } };
class Store {
public:
    explicit Store(const std::string& path);
    void put(const std::string& key, const std::string& value);
    std::optional<std::string> get(const std::string& key) const;
    Entry* find(const std::string& key);                 // nullptr on miss
    void compact();                                      // may throw
    void on_change(std::function<void(const std::string&)> f);
    template <class T> T get_as(const std::string& key) const;
    int scaled(int factor = 10) const;
    static Store* open_default();
};
std::string greet(const std::string& who);
}
```

```dream
// src/kvstore.dream
module kvstore;

@cpp("kv.hpp", "kv::Store")
public class Store {
    extern constructor(path: string);
    extern fun put(key: string, value: string): void;
    extern fun get(key: string): Option<string>;
    extern fun find(key: string): Option<Entry>;       // borrowed; keeps the Store alive
    extern fun compact(): Result<bool, string>;        // Ok(true), or Err(e.what())
    extern fun on_change(f: fun(string): void): void;  // capturing closures are fine
    @cpp_name("get_as<int>") extern fun get_int(key: string): int;
    extern fun scaled(): int;                          // C++ fills in `factor`
    @owned static extern fun open_default(): Store;
}

@cpp("kv.hpp", "kv::Entry")
public class Entry {
    @cpp_name("name") extern fun key(): string;
}

@cpp("kv.hpp", "kv::greet")
extern fun greet(who: string): string;
```

- **`@cpp("header")`** or **`@cpp("header", "ns::Name")`** goes on a class, a value struct, or a free
  `extern fun`. The header resolves against the set's include path, and the second argument
  defaults to the Dream name.
- **Members** are `extern constructor(...)`, instance `extern fun`, and `static extern fun`. A
  `@cpp` class holds only `extern` members. Put Dream helpers in an `extend Store { ... }` block or
  a wrapping class.
- **Visibility.** `@cpp` members and free functions are public, or `internal` when marked so. The
  class's own `public` controls whether other files can see it.
- **`@cpp_name("expr")`** renames a member. It also selects a template instantiation
  (`get_as<int>`).
- **Overloads** are ordinary Dream overloads; C++ picks the matching one.
- **Default arguments.** Declare fewer parameters and C++ fills in the rest. Dream-side defaults
  and variadics are rejected.
- `@cpp` members cannot be generic, `async`, operators, indexers, or accessors. Bind a named C++
  function instead.

### Bridgeable types

| Dream | C++ |
|-------|-----|
| numbers, `bool`, `char`, `byte` | the matching arithmetic type |
| `string` | `std::string`, `const std::string&`, `std::string_view`, `const char*` |
| `Option<string>` | `std::optional<std::string>`, or a nullable `const char*` |
| `CPtr`, `Option<CPtr>` | any pointer |
| a `@cpp` class, `Option<Class>` | `T`, `T&`, `T*`, `std::unique_ptr<T>`, `std::optional<T>` |
| a `@cpp` value struct | the struct by value or `const T&`; `ref` for `T&` |
| `T[]` of numbers | `std::span<const T>` or `const std::vector<T>&` |
| `fun(...)` | `std::function<...>` |
| `ref x: T` (scalars, structs) | `T&` |
| `Result<T, string>` result | `T`, with a thrown exception becoming `Err(e.what())` |

Anything else is a diagnostic on the declaration.

### Ownership

The real C++ return type decides ownership, and the shim resolves it at compile time:

- **By value, `std::unique_ptr<T>`, or `std::optional<T>`: owned.** The object moves to the heap and
  is deleted once, when the Dream object is released.
- **`T*` or `T&`: borrowed.** The Dream object holds a strong reference to the object it came from,
  so an `Entry` from `store.find(...)` keeps the `Store` alive.
- **`@owned` on a member returning `T*`** takes ownership instead (factories).

A `NULL` pointer returned where the declaration is not `Option` traps with the member's name.

### Exceptions

Every shim call catches C++ exceptions. With a `Result<T, string>` return, an exception becomes
`Err(e.what())`. Otherwise the program traps with the message. A `void` C++ member that may throw
declares `Result<bool, string>` and yields `Ok(true)`, matching the stdlib convention.

### Callbacks

`fun(...)` parameters map to `std::function`. The shim retains the closure for as long as the
`std::function` lives and releases it when C++ destroys it, so capturing closures need no
`NativeCallback`. The same thread rule applies: calls must arrive on a Dream thread.

### Structs

A `@cpp` value struct holds only scalar fields. The shim checks its layout with `static_assert`
on `sizeof` and every `offsetof`. A mismatch fails the C++ compile, reported as:

```
@cpp struct 'Point': sizeof(geom::Point) differs from the Dream declaration (16 bytes)
```

```dream
@cpp("kv.hpp", "geom::Point")
public struct Point {
    public x: double;
    public y: double;
}
```

### Checked by the C++ compiler

The shim uses `#line` directives, so a declaration that does not match the header fails the build
at the `.dream` line that declared it:

```
src/kvstore.dream:6:5: error: no member named 'value' in 'm::Box'
```

The generated `shim.cpp` and `dream_bridge.hpp` are written next to the build output
(`target/<profile>/native-c/<set>/`) for inspection. A shim covers every `@cpp` declaration of a
set the program uses.

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

`@c("lib", ...)` with a library that is not a native set links `-l<lib>`. Search order:

1. `native/<lib>` next to the source (vendored shared libraries).
2. The directory containing the source, then the current working directory.
3. Standard system directories:
   - macOS: `/opt/homebrew/lib`, `/usr/local/lib`, `/opt/local/lib`, `/usr/lib`
   - Linux: `/usr/local/lib`, `/usr/lib/x86_64-linux-gnu`, `/usr/lib/aarch64-linux-gnu`,
     `/usr/lib64`, `/usr/lib`, `/lib`
   - Windows: `%WINDIR%\System32`
4. The OS loader's own path (`DYLD_FALLBACK_LIBRARY_PATH` / `LD_LIBRARY_PATH`).

[`sample/sqlite/`](../../../sample/sqlite) binds the system libsqlite3 this way:

```bash
dream run sample/sqlite/db.dream
```

## Compared to `@js` and `@runtime`

| Concern | `@js("mod", "field")` | `@runtime("name")` | `@c` / `@cpp` |
|---------|-----------------------|--------------------|---------------|
| Host | JS (Node / browser) | Dream runtime (WASM + native) | Native builds |
| Async | `async` + `Promise` bridge | `async` + host future | Not supported; C runs on the caller's thread |
| Out-params | Return a wrapper struct / tuple | Return a wrapper struct / tuple | `ref` parameters |
| Callbacks into Dream | `fun(...)` values | host-defined | `fun` pointers, `NativeCallback`, `std::function` |
| Null | `Option` | `Option` | `Option` |
