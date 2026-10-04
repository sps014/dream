# Native libraries

Mark functions with `@export`, then build either library kind:

```dream
@export
fun add(a: int, b: int): int { return a + b; }
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

Use `output-type = "cdylib"` for a shared C ABI library. The value is a single string.
The default source is `src/mylib.dream` (hyphens and dots in package names become underscores).
Build with `dreamer build` or `dream build`; an explicit source path and `-o` may override
input and output paths without changing the manifest's output kind.

```sh
dreamer build
dream build --release
dream build src/mylib.dream -o target/mylib.a
```

These builds reject `main`. Exports must be synchronous, non-generic, non-variadic functions
with distinct C identifiers; runtime and compiler names are reserved. Public Dream visibility
alone does not export a C symbol. Library outputs link for the host by default.
`dream --target TRIPLE` links the configured library for that target; add `--object` to emit only its interface and an unlinked object.
Target builds require matching capability libraries and SDKs;
[mobile packaging](mobile-packaging.md) consumes target-built libraries.
Native library output selection belongs to `[lib].output-type`; `--crate-type` only selects
validation for intermediate output or a different target.

Outputs include `<stem>.h`, `<stem>.abi.json`, `<stem>.opt.ll` and either an archive
(`.a`, or `.lib` on Windows) or a shared library (`.dylib`, `.so`, or `.dll`). Windows shared
libraries also produce an import `.lib`. Static libraries provide `<stem>.link.json`: append its
`link_args` array when linking the archive into a C, Swift or Kotlin host. Required Dream host
capability libraries remain dynamic dependencies and must be available to the host application.

The generated header contains both the exported declarations and the embedding API. For example:

```c
#include "lib.h"
int main(void) {
    dream_thread_attach();
    int answer = add(20, 22);
    dream_thread_detach();
    return answer == 42 ? 0 : 1;
}
```

Serialize the first exported call, which initializes module globals. Attach every calling thread
and detach it before it exits. Respect the embedding API's thread and callback ownership rules;
ordinary reference counts are not atomic.

The exported ABI uses 32-bit integers for `int`, `uint`, `bool`, `byte`, `char` and enums,
64-bit integers for `long`/`ulong`, pointer-width integers for `isize`/`usize`, and native
`float`/`double` for floating point. Managed objects, strings, arrays, function boxes and value
structs cross this boundary as opaque pointers, rather than C aggregates. Value-struct results
are heap boxes. Unmarked managed parameters consume one reference; `borrow` and `ref` parameters
borrow. Returned managed references own one count, which the host releases with `dream_release`.
The C header records the lowered ABI; these declarations are not the by-value C struct ABI.

Panics never unwind into the host. A panic hook receives the message and source location;
returning from the hook still aborts. Locations identify the library's own source statement,
such as `mylib/src/lib.dream:12`; stdlib and dependency panics identify the library line that
called them. Paths are relative to the package root and prefixed with its manifest name.
C callers supply no hidden location argument. Prebuilt Dream-to-Dream linking is not supported.
