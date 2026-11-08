---
title: C++ libraries
summary: "Wrap a C++ library: accepted types, ownership, callbacks, and errors."
icon: material/language-cpp
status: stable
targets: [native, web, node]
---

# Connect to a C++ library

Use a package wrapper to call a C++ library. This guide covers accepted types, ownership, callbacks, and errors.

!!! glance "At a glance"

    ```dream
    @cpp("kv.hpp", "geom::Point")
    public struct Point {
        public x: double;
        public y: double;
    }
    ```

    - Map C++ types and functions to Dream declarations with `@cpp`.
    - A package wrapper owns the C++ library and its build settings.

[Back to overview](c.md)

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
import system.interop;

// src/kvstore.dream
module kvstore;

@cpp("kv.hpp", "kv::Store")
public class Store {
    extern constructor(path: string);
    extern fun put(key: string, value: string): void;
    extern fun get(key: string): Option<string>;
    extern fun find(key: string): Option<Entry>; // borrowed; keeps the Store alive
    extern fun compact(): Result<Unit, string>; // Ok(Unit()), or Err(e.what())
    extern fun on_change(f: fun(string): void): void; // capturing closures are fine

    @cpp_name("get_as<int>")
    extern fun get_int(key: string): int;

    extern fun scaled(): int; // C++ fills in `factor`

    @owned
    static extern fun open_default(): Store;
}

@cpp("kv.hpp", "kv::Entry")
public class Entry {
    @cpp_name("name")
    extern fun key(): string;
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

The C++ return type decides ownership:

- **By value, `std::unique_ptr<T>`, or `std::optional<T>`: owned.** Dream deletes the object once, when your program no longer uses it.
- **`T*` or `T&`: borrowed.** The Dream object keeps its source alive, so an `Entry` from `store.find(...)` keeps the `Store` alive.
- **`@owned` on a member returning `T*`** takes ownership instead. Use this for factories.

A `NULL` pointer returned where the declaration is not `Option` traps with the member's name.

### Exceptions

Every call catches C++ exceptions. A `Result<T, string>` becomes `Err(e.what())`, and any other return stops the program with that message.

A `void` C++ member that may throw should return `Result<Unit, string>` and yield `Ok(Unit())`.

### Callbacks

`fun(...)` parameters map to `std::function`, and Dream keeps the closure alive for as long as that `std::function` does. The call must arrive on a Dream thread, including when the closure captures values.

### Structs

A `@cpp` value struct holds only scalar fields. Its size and field offsets are checked against the C++ type, and a mismatch fails the C++ compile:

```
@cpp struct 'Point': sizeof(geom::Point) differs from the Dream declaration (16 bytes)
```

```dream
import system.interop;

@cpp("kv.hpp", "geom::Point")
public struct Point {
    public x: double;
    public y: double;
}
```

### Checked by the C++ compiler

A declaration that does not match the header fails the build at the `.dream` line that declared it:

```
src/kvstore.dream:6:5: error: no member named 'value' in 'm::Box'
```

`shim.cpp` and `dream_bridge.hpp` are written next to the build output (`target/<profile>/native-c/<set>/`) when you need to inspect them. They include every `@cpp` declaration the program uses.
