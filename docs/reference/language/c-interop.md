# C Interop

Dream can call native C libraries through the `@c(...)` attribute. This is **native-only**: a program with a `@c` extern will not run on Node or in the browser. `@c` automatically restricts the extern to the native target — you don't need to spell `@native` yourself.

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
- **Do not** combine `@c` with `@js` on the same declaration.

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

After the call, `db` holds the value C wrote. Value-struct out-params work the same way.

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

Both `@marshal` and `@c_call` require `@c` on the same declaration.

## Passing structs

Pass C structs as `@unmanaged` value structs. Mark them `@packed` when the C header
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

Only `@unmanaged` value structs may appear in `@c` signatures — classes and unions are rejected,
because they are heap references C does not understand.

## Callbacks

Captureless Dream `fun` values passed to `@c` parameters become native function pointers for the
duration of the call:

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

1. `native/<lib>` **next to** the source (perfect for vendored copies).
2. The directory containing the source.
3. The current working directory.
4. Standard system directories:
   - macOS — `/opt/homebrew/lib`, `/usr/local/lib`, `/opt/local/lib`, `/usr/lib`
   - Linux — `/usr/local/lib`, `/usr/lib/x86_64-linux-gnu`, `/usr/lib/aarch64-linux-gnu`,
     `/usr/lib64`, `/usr/lib`, `/lib`
   - Windows — `%WINDIR%\System32`
5. The OS loader's own search path (`DYLD_FALLBACK_LIBRARY_PATH` / `LD_LIBRARY_PATH` / system stubs).

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

`dreamer pack` copies the libraries your `@c` externs need.

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

To ship a single native executable:

```bash
cd sample/sqlite
dreamer pack                 # → target/pack/sqlite-demo-<os>-<arch>
./target/pack/sqlite-demo-macos-arm64
```

Both work out-of-the-box on macOS and on Linux distributions that ship libsqlite3 in a standard
library dir; on hosts that don't, drop a `native/libsqlite3.*` into `sample/sqlite/` (or set
`DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH`). The packed binary still needs the system sqlite3
shared library at run time unless you vendor it.

## Host helpers for C pointers

SQLite (and most C libraries) pass `char*` / `T*` that live in the **host** address space, not
inside the Dream program. From a Dream callback, use:

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
| Host                      | JS (Node / browser)                 | Native (`dream run`)                      |
| Fixed signature           | Yes                                 | Yes                                        |
| Automatic marshaling      | Yes                                 | Yes                                        |
| Async                     | `async` + `Promise` bridge          | Not supported (call C on the caller thread) |
| Out-params                | Return a wrapper struct / tuple     | Dream `ref` parameter                      |
| Callbacks (into Dream)    | `fun(...)` values marshalled to JS  | Captureless `fun(...)` → native callback   |

See [JS Interop](interop.md) for the JS side.
