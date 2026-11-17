---
title: Native libraries
summary: "Build shared or static libraries from `@export` functions."
icon: material/library-outline
---

# Native libraries

Mark functions with `@export`, then build either library kind:

```dream
@export
fun add(a: int, b: int): int {
    return a + b;
}
```

Select one native library output in `dream.toml`:

```toml
[package]
name = "mylib"
version = "0.1.0"
type = "lib"

[lib]
output-type = "staticlib"
```

| `output-type` | Builds |
|---|---|
| `"staticlib"` | A static archive linked into the host program |
| `"cdylib"` | A shared C ABI library loaded at run time |

Use `output-type = "cdylib"` for a shared C ABI library. The value is one string, and `[lib]` is allowed only on a `type = "lib"` package with no `icon` and no `targets` other than `native`.

The default source is `src/mylib.dream` (hyphens and dots in the package name become underscores). `dreamer build` and `dream build` both work, and a source path plus `-o` can override the input and output without changing the manifest's output kind.

```sh
dreamer build
dream build --release
dream build src/mylib.dream -o target/mylib.a
```

These builds reject `main`. Exports must be synchronous, non-generic, non-variadic functions with distinct C identifiers.

Runtime and compiler names are reserved, and public Dream visibility alone does not export a C symbol. Library outputs link for the host by default.

`dream --target TRIPLE` links for that target, and `--object` emits the interface plus an unlinked object. Target builds need matching capability libraries and SDKs ([mobile packaging](mobile.md)).

`[lib].output-type` chooses the library kind. `--crate-type` only selects validation for intermediate output or a different target.

| File | Produced for | Contents |
|---|---|---|
| `<stem>.h` | Both kinds | C declarations for the exports and the embedding API |
| `<stem>.abi.json` | Both kinds | Machine-readable export inventory |
| `<stem>.a` (`.lib` with MSVC) | `staticlib` | The static archive |
| `<stem>.link.json` | `staticlib` | `link_args` to append when linking the archive |
| `<stem>.dylib` / `.so` / `.dll` | `cdylib` | The shared library (macOS / Linux / Windows) |
| `<stem>.lib` | `cdylib` on Windows | Import library for the `.dll` |
| `<stem>.opt.ll` | Either, with `--emit-opt-ir` | Optimized LLVM IR |

When linking a static archive into a C, Swift or Kotlin host, append the `link_args` array from
`<stem>.link.json`. Required Dream host capability libraries remain dynamic dependencies and must
be available to the host application.

## Build and run a C consumer

The generated header contains the exports and the embedding API. Build the sample
with both link modes using a C compiler and Python:

```sh
python3 sample/native_embed/run.py
```

Run this command from the Dream repository. Set `DREAM` or `CC` if the executables
you want are outside your search path. The consumer prints `42` for each link mode.
It checks the embedding ABI version, calls a scalar export, reads a managed string
through `dream_string_units`, and releases the string with `dream_release`.

The consumer includes the generated header for the library containing `add` and
`greeting`. The latter returns a Dream string with one reference owned by C.

```c
#include "native_embed.h"
#include <stdint.h>

int main(void) {
    if (dream_embed_abi_version() != DREAM_EMBED_ABI_VERSION) {
        return 1;
    }

    dream_thread_attach();
    int32_t sum = add(20, 22);
    void *text = greeting();
    int valid = sum == 42 && dream_string_length(text) == 5;
    dream_release(text);
    dream_thread_detach();
    return valid ? 0 : 1;
}
```

Serialize the first exported call so module globals initialize once. Attach each
calling thread with `dream_thread_attach()` and detach it before exit. For a shared
build, configure the host loader to find the Dream library and any required host
capability libraries. The sample uses an rpath on macOS and Linux and places the
DLL beside the consumer on Windows. For a static build, append the generated
`link_args` to the host’s link command.

The exported ABI uses 32-bit integers for `int`, `uint`, `bool`, `byte`, `char`, and enums, and 64-bit integers for `long` and `ulong`. `isize` and `usize` are pointer-width, and `float` and `double` stay native floating-point values.

Managed objects, strings, arrays, function boxes, and value structs cross as opaque pointers, not as C structs. A plain managed parameter is consumed by the call, `borrow` and `ref` are not, and the host releases a returned value with `dream_release`.

Panics never unwind into the host. A panic hook receives the message and source location, and returning from the hook still aborts.

Locations name the library statement, such as `mylib/src/lib.dream:12`. A panic in the standard library or a dependency names the library line that called it, with a path relative to the package root and prefixed by the package name.

C callers pass no hidden location argument. Prebuilt Dream-to-Dream linking is not supported.
