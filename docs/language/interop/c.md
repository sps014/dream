---
title: C
summary: Call C from Dream, and let C call back into a Dream program.
icon: material/language-c
status: stable
targets: [native, web, node]
---

# C interop

Declare a C function in Dream and call it like any other function. Put the C source in the package's `native/` directory, or link a system library by name.

```dream
import system.interop;

@c("m", "cbrt")
extern fun cube_root(x: double): double;
```

C++ uses [`@cpp`](cpp.md). A library that calls back into Dream uses [`NativeCallback`](native-callbacks.md).

The compiler does not read C headers. You write the Dream declaration yourself, so the editor can complete and jump to it. Dream checks that the declaration matches the compiled C function. A mismatch is a build error.

## Where the C lives

```
ticker/
├── dream.toml
├── src/ticker.dream
└── native/
    ├── include/ticker.h
    └── ticker.c
```

Files under `native/` are one set, named after the package. `native/include/` is on the include path. `.c` files compile as C11. `.cpp`, `.cc`, and `.cxx` compile as C++20. On Apple, `.m` is Objective-C and `.mm` is Objective-C++. Other targets reject those two.

A set is built only when the program calls into it. Change a source and the next build recompiles that file.

```toml
[native.ticker]
defines = ["TICKER_THREADSAFE=1"]
libs = ["m"]

[native.ticker.macos]
frameworks = ["Security"]
```

| Key | Meaning |
|-----|---------|
| `sources` | Extra source files, relative to the package |
| `include` | Extra include directories |
| `defines` | Macros passed to the compiler |
| `cflags` | Extra compiler flags. `-std`, `-O`, and `-g` are reserved |
| `frameworks` | Apple frameworks |
| `libs` | System libraries (`-l`) |
| `exclude` | Globs to leave out. `*` stays in one directory. `**` crosses directories |
| `lto` | `true` by default. Release builds compile the set with the rest of the program. Set `false` for assembly or a function that is not the platform C calling convention |

`macos`, `linux`, `windows`, and `wasm` tables add keys for that target only. A WebAssembly build does not use the host operating-system table.

## Declaring a function

```dream
import system.interop;

@c
extern fun ticker_new(name: string): CPtr;

@c("m", "cbrt")
extern fun cube_root(x: double): double;
```

- Bare `@c` uses this package's `native/` set. The Dream name is the C name.
- `@c("lib")` uses another set or a system library. `@c("lib", "symbol")` also picks the C name.
- One declaration cannot also be `@js`, `@runtime`, or `@intrinsic`.
- `@c` functions are not generic and not variadic. Write one declaration per argument count.

## Types

| Dream | C |
|-------|---|
| `int`, `long`, `float`, `double` | `int32_t`, `int64_t`, `float`, `double` |
| `isize`, `usize` | `intptr_t`, `uintptr_t` (`usize` also matches `size_t`) |
| `bool`, `char`, `byte` | `bool`, `char`, `uint8_t` |
| `sbyte`, `short`, `ushort` | `int8_t`, `int16_t`, `uint16_t` |
| `string` argument | `const char *`, UTF-8, valid for the call |
| `borrow string` | `dream_str_view` `{ units, len }`, valid for the call |
| `string` result | `const char *`, copied into a Dream string |
| `@owned` `string` result | a Dream string that C built |
| `byte[]` or `T[]` argument | pointer to the first element, or `NULL` when empty |
| `byte[]` or `T[]` result | the Dream array C built |
| `CPtr`, `Option<CPtr>` | `void *`. `NULL` is `None` for `Option` |
| `Option<string>` | `const char *`. `NULL` is `None` |
| value struct of numbers | the C struct, same fields in the same order |
| `ref x: T` | `T *` |
| `fun(...)` | a function pointer |
| `NativeCallback<F>` | a function pointer and a `void *` |
| `OwnedCPtr` with `@owned("free_fn")` | `void *` that Dream later passes to `free_fn` |

`long` is always 64 bits. Use `usize` and `isize` for `size_t` and `ptrdiff_t`.

`char` in a struct is C `char`. A `ref` to a Dream `char` is not a `char *`: Dream stores a character in four bytes. Pass a `byte[]` when C wants a byte buffer.

Classes, unions, and arrays of objects cannot cross this boundary. Pass the fields C needs.

## Strings, arrays, and pointers

A normal `string` argument is a temporary UTF-8 copy. A normal `string` result is copied from a `const char *` that C still owns. `NULL` is an error unless the result is `Option<string>`.

`borrow text: string` does not copy. C receives `dream_str_view`, a pointer to UTF-16 units and a length. Do not store that pointer.

`@marshal(StringEncoding.Utf16)` uses UTF-16 (`char16_t *`, Windows `LPWSTR`) for the copied form. It is not `wchar_t`.

```dream
import system.interop;

@c("user32", "MessageBoxW")
@marshal(StringEncoding.Utf16)
extern fun MessageBoxW(hwnd: CPtr, text: string, caption: string, flags: int): int;
```

An array argument is a pointer to the first element for the duration of the call. Pass the length yourself:

```dream
@c
extern fun checksum(data: byte[], len: int): int;
```

An array result is a Dream array. C builds it and returns the pointer from `dream_bytes_new` or `dream_scalar_array_new`. `dream_array_data` is the first byte. Dream frees the array.

```c
#include <dream_embed.h>

void *take_bytes(int32_t n) {
    void *array = dream_bytes_new(n);
    unsigned char *bytes = dream_array_data(array);
    for (int32_t i = 0; i < n; i++) {
        bytes[i] = (unsigned char)(i + 1);
    }
    return array;
}
```

```dream
@c
extern fun take_bytes(n: int): byte[];
```

`@owned` on a `string` result means C returns a Dream string from `dream_string_from_utf8` or `dream_string_from_utf16`. Dream owns that string. A raw pointer that C allocated uses `OwnedCPtr` and `@owned("free_fn")` instead. Dream calls `void free_fn(void *)` when the value is dropped.

```dream
@c
@owned("ticker_free")
extern fun ticker_new(name: string): OwnedCPtr;
```

`CPtr` is an opaque pointer: `CPtr.null()`, `is_null()`, `address()`, and `offset(bytes)`. Use `ref` for an out-parameter:

```dream
@c("sqlite3")
extern fun sqlite3_open(path: string, ref db: CPtr): int;
```

`Ffi` reads and writes memory through a `CPtr`. The index is an `isize` byte offset. A null pointer stops the program. `Ffi.read_cstring` returns `Option.None` for `NULL`.

A value struct used with `@c` contains only numbers, `bool`, `char`, `byte`, or more of the same structs. Field order matches the C struct. Add `@packed` when the C type is packed.

## Async

An `async` `@c` function receives the Dream future as its first C argument and returns `int`. Return `0` when the work finished before the return. Return anything else when another thread will finish it later.

```c
int32_t add_later(void *future, int32_t a, int32_t b) {
    dream_future_complete_u64(future, (uint64_t)(a + b));
    return 0;
}
```

```dream
@c
extern async fun add_later(a: int, b: int): int;
```

| Call | Result |
|------|--------|
| `dream_future_complete_u64(future, bits)` | a number, `bool`, or `CPtr` bits |
| `dream_future_complete_ref(future, value)` | a Dream string or array that C built. Dream owns it |
| `dream_future_complete_error(future, code, message)` | the awaiting Dream code stops with `code: message` |
| `dream_future_on_cancel(future, fn, data)` | `fn(data)` runs when Dream cancels the call |

A second completion does nothing. Completing and also returning `0` is only correct when the completion happened on this thread before the return.

`dream_post(owner, fn, data)` runs `fn(data)` on that Dream thread. `dream_current_owner()` is the owner of the thread you are on. On that thread, `dream_post` runs `fn` immediately.

## Callbacks

A named function, or a lambda written at the call that captures nothing, becomes a C function pointer. It may be called from any thread. A thread that C created calls `dream_thread_attach` first and `dream_thread_detach` before it exits.

`NativeCallback` can capture values. C receives a function pointer and a `void *`. It may be called only on the Dream thread that created it. Keep the object alive while C can call it. See [Callbacks](native-callbacks.md).

## System libraries

`@c("sqlite3", "sqlite3_open")` links `-lsqlite3` when `sqlite3` is not a native set. Dream looks in this order:

1. `native/<lib>` next to the source.
2. The source directory, then the current directory.
3. The usual system directories (`/opt/homebrew/lib`, `/usr/local/lib`, `/usr/lib`, and the Windows system directory).
4. The loader path (`DYLD_FALLBACK_LIBRARY_PATH` or `LD_LIBRARY_PATH`).

`[package] links = "sqlite3"` says this package provides that library. Only one package in the program may claim a given name.

## WebAssembly

The same `native/` sources and `@c` declarations compile into a wasm32 program, with WASI libc and libm. A desktop `.so` or `.dylib` does not. Use `@js` for browser and Node services.

`usize` and `isize` still match `size_t` and `ptrdiff_t`. A signature that does not match the compiled function fails the build.

Workers are Dream tasks. WASI `pthread_create` is not available. A callback stays on the instance that created it.

## Embedding

`#include <dream_embed.h>` works from every `native/` file.

`dream_embed_abi_version()` returns `DREAM_EMBED_ABI_VERSION` (1). Compare it before calling anything else.

`dream_thread_attach` can nest. Each attach needs a detach. Detach does not shut down a Dream worker. `dream_callback_release` is safe from any thread. The Dream thread that created the callback applies the release.

`dream_retain` and `dream_release` count a Dream reference. Debug builds reject them on a thread that has not attached. A panic hook sees at most 1024 bytes of the message.

`dream_set_platform` checks `dream_platform.size`. `0` is accepted. Any other value must be `sizeof(dream_platform)`.

## What v1 does not do

- Read C or C++ headers for you.
- C variadic functions. Write one declaration per argument count.
- A callback from one WebAssembly instance into another.
- `@c` combined with `@js` or `@runtime`.
