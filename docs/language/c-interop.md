# C Interop

Dream can call native C libraries directly on the wasmtime host through the `@c(...)` attribute.
The compiler emits a WebAssembly `import` for every `@c` extern, and the CLI runtime resolves the
symbol at instantiation time via `libloading` + `libffi`, marshalling arguments and returns
without any hand-written trampoline.

C interop is **native-only**: a program that carries a `@c` extern will not run on Node or in the
browser (there is no libc to call). The `@c` attribute automatically restricts the enclosing
extern to the `native` runtime target — you don't need to spell `@native` yourself.

## Declaring an extern

```dream
@native
@c("sqlite3", "sqlite3_open")
extern fun sqlite3_open(path: string, ref db: long): int;
```

- The first `@c` argument is the library name (`sqlite3` → `libsqlite3.dylib` / `.so` / `.dll`).
- The second is the exported C symbol.
- Parameters and returns use ordinary Dream types (`int`, `long`, `float`, `double`, `bool`,
  `byte`, `string`, `byte[]`, or an `@unmanaged` value struct).
- **Do not** combine `@c` with `@js` on the same declaration — the compiler will reject it.

## Out-parameters use `ref`

C APIs often return values by writing through a pointer. Dream models that with a `ref`
parameter — no separate attribute is needed:

```dream
@native
@c("sqlite3", "sqlite3_open")
extern fun sqlite3_open(path: string, ref db: long): int;

fun main(): void {
    let db: long = 0L;
    let rc = sqlite3_open("app.db", ref db);   // C sees `sqlite3 **db` and writes through it.
}
```

The compiler encodes the parameter as `out_long` / `out_int` in the `.abi.json`, and the WASM
import receives an `i32` linear-memory address. On the way in, the host trampoline hands the C
function a real host pointer; on the way out, it copies the C-written value back to the Dream box
so the callee's assignment to `ref db` becomes visible.

Value-struct out-params are supported the same way (`out_struct:Name`), backed by a per-call
scratch buffer sized from the ABI's `structs` map.

## String marshaling

By default, `string` parameters are copied into a fresh C `NUL`-terminated byte buffer and
released after the call. To pass UTF-16 (Windows `LPWSTR`, WinAPI-style) instead:

```dream
@native
@c("user32", "MessageBoxW")
@marshal("lpwstr")
extern fun MessageBoxW(hwnd: long, text: string, caption: string, flags: int): int;
```

`@marshal("lpstr")` is the default and never needs to be spelled out.

## Calling convention

Cdecl is the default on all supported hosts. Use `@c_call("stdcall")` for Win32 APIs that use the
`stdcall` convention:

```dream
@native
@c("user32", "GetSystemMetrics")
@c_call("stdcall")
extern fun GetSystemMetrics(index: int): int;
```

Both `@marshal` and `@c_call` require `@c` on the same declaration; the compiler rejects them
otherwise.

## Passing structs

C-ABI structs are declared as `@unmanaged` value structs and marked `@packed` when the C header
uses `#pragma pack(1)` / `__attribute__((packed))`:

```dream
@packed
struct Point {
    x: int;
    y: int;
}

@native
@c("mylib", "point_distance")
extern fun point_distance(a: Point, b: Point): double;
```

The `.abi.json` `structs` map records the layout the host needs to marshal the value:

```json
"structs": {
  "Point": { "size": 8, "align": 1, "packed": true, "fields": [
    { "name": "x", "offset": 0, "ty": "int" },
    { "name": "y", "offset": 4, "ty": "int" }
  ]}
}
```

Only `@unmanaged` value structs may appear in `@c` signatures — the compiler rejects a class or a
union in a `@c` signature, because heap references are Dream-managed pointers the C ABI knows nothing
about.

## Callbacks

Captureless Dream `fun` values passed to `@c` parameters become native function pointers for the
duration of the call. The host installs a libffi trampoline that re-enters the WebAssembly module
through `__indirect_function_table`.

```dream
fun row_cb(arg: long, argc: int, argv: long, cols: long): int {
    return 0; // continue
}

@native
@c("sqlite3", "sqlite3_exec")
extern fun sqlite3_exec(
    db: long,
    sql: string,
    callback: fun(long, int, long, long): int,
    arg: long,
    ref errmsg: long
): int;

sqlite3_exec(db, "SELECT 1", row_cb, 0L, ref err);
```

- Callbacks must be **captureless** (same rule as JS callbacks).
- Null C callbacks: declare the parameter as `long` and pass `0L` when you need a null function
  pointer (a `fun` value cannot be null).

## Auto-linking libraries

The runtime searches for the requested library in this order:

1. `native/<lib>` **next to** the `.dream` / `.wat` source (perfect for vendored copies).
2. The directory containing the `.wat` itself.
3. The current working directory.
4. Standard system directories:
   - macOS — `/opt/homebrew/lib`, `/usr/local/lib`, `/opt/local/lib`, `/usr/lib`
   - Linux — `/usr/local/lib`, `/usr/lib/x86_64-linux-gnu`, `/usr/lib/aarch64-linux-gnu`,
     `/usr/lib64`, `/usr/lib`, `/lib`
   - Windows — `%WINDIR%\System32`
5. As a last resort, `libloading::Library::new("lib<name>.dylib")` (etc.) lets the OS loader walk
   its own search path (`DYLD_FALLBACK_LIBRARY_PATH` / `LD_LIBRARY_PATH` / system stubs).

To ship a self-contained program, drop the platform-appropriate shared library into a `native/`
folder next to the source:

```
sample/sqlite/
├── raw.dream
└── native/
    ├── libsqlite3.dylib   # macOS
    ├── libsqlite3.so      # Linux
    └── sqlite3.dll        # Windows
```

The `.abi.json` also emits a `c_libs` array listing every library referenced by a live `@c`
extern, so packaging tooling (`dreamer pack`, custom scripts) can copy the exact set of native
dependencies without re-parsing sources.

## End-to-end example

The `sample/sqlite/` folder contains two runnable samples:

- `raw.dream` — direct `@c` bindings to libsqlite3, opening an
  in-memory database and executing DDL/DML.
- `db.dream` — a small `Database` class that creates `./demo.db` in
  the process working directory, inserts rows, and prints `SELECT` results via a C callback that
  uses `dream_ffi.read_ptr` / `dream_ffi.read_cstring` to read host `char**` values.

Run either with:

```bash
dream run sample/sqlite/raw.dream
dream run sample/sqlite/db.dream
```

Expected `db.dream` / packed `sqlite-demo` output includes the selected rows:

```text
opened demo.db (cwd=…)
DROP rc=0
CREATE rc=0
INSERT rc=0
--- SELECT ---
1 | apple
2 | pear
3 | kiwi
SELECT rc=0
done
```

To ship a single native executable (embeds the `.wasm` + `.abi.json`, and links `sqlite3`
via `c_libs`):

```bash
cd sample/sqlite
dreamer pack                 # → target/pack/sqlite-demo-<os>-<arch>
./target/pack/sqlite-demo-macos-arm64
```

Both work out-of-the-box on macOS and on Linux distributions that ship libsqlite3 in a standard
library dir; on hosts that don't, drop a `native/libsqlite3.*` into `sample/sqlite/` (or set
`DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH`). The packed binary still needs the system/`-lsqlite3`
shared library at run time unless you vendor it.

## Host helpers for C pointers

SQLite (and most C libraries) pass `char*` / `T*` that live in the **host** address space, not
Dream linear memory. From a Dream callback, use:

```dream
@native
@js("dream_ffi", "read_ptr")
extern fun ffi_read_ptr(base: long, index: int): long;

@native
@js("dream_ffi", "read_cstring")
extern fun ffi_read_cstring(ptr: long): string;
```

`read_ptr(base, i)` loads `*((void**)base + i)`; `read_cstring` copies a NUL-terminated UTF-8
C string into a Dream `string`.

## Compared to `@js`

`@c` and `@js` are mutually exclusive on a single extern (they name incompatible hosts), but they
share the same *shape* on the Dream side:

| Concern                   | `@js("mod", "field")`               | `@c("lib", "symbol")`                     |
|---------------------------|-------------------------------------|-------------------------------------------|
| Host                      | JS (Node / browser)                 | Native (wasmtime + `libloading` + `libffi`) |
| Fixed signature           | Yes                                 | Yes                                        |
| Automatic marshaling      | Yes, via `.abi.json`                | Yes, via `.abi.json` (`kind: "c"`)         |
| Async                     | `async` + `Promise` bridge          | Not supported (call C on the caller thread) |
| Out-params                | Return a wrapper struct / tuple     | Dream `ref` parameter                      |
| Callbacks (into Dream)    | `fun(...)` values marshalled to JS  | Captureless `fun(...)` → native trampoline |

See [JS Interop](interop.md) for the JS side.
