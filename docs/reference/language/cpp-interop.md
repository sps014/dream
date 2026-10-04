# Connect to a C++ library

Use a package wrapper to call a C++ library. This guide covers accepted types, ownership, callbacks, and errors.

[Back to overview](c-interop.md)

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
