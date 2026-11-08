---
title: Classes and structs
summary: Group fields and methods into shared objects or copied values.
icon: material/cube-outline
status: stable
targets: [native, web, node]
---

# Classes & Structs

Classes and structs group fields and methods. A class is shared on assignment, and a struct is copied. Pick the one your data needs.

!!! glance "At a glance"

    ```dream
    class Point(x: int, y: int);

    let p1 = Point(3, 4);
    let p2 = p1; // same object
    ```

    - A class instance is shared when assigned; a struct is copied.
    - Both group fields and methods, and can implement interfaces.

## Kinds of type declarations

| Declaration | Assignment | Use it for |
|-------------|------------|------------|
| `class` | Shares the same object | Data with identity and a lifetime |
| `struct` | Copies the value | Small, copyable bundles of fields |
| `ref struct` | Copies the value | Views like `Span<T>` that must never leave the current stack frame |
| `shared class` | Shares the same object | Objects used from more than one task, guarded with `lock` |
| `static class` | Cannot be created | A namespace of static helpers |
| `sealed class` / `sealed struct` / `sealed enum` | Same as without `sealed` | Types whose methods `extend` blocks must not add to |

## Member modifiers

| Modifier | Applies to | Effect |
|----------|------------|--------|
| (none) | Members | Private: reachable only from the type's own methods |
| `internal` | Members, header parameters | Reachable anywhere in the same module |
| `public` | Members | Reachable wherever the type is reachable |
| `static` | Fields, methods, properties | Belongs to the type rather than to an instance |
| `weak` / `unowned` | Fields | Does not keep the target alive (see [Reference cycles](../memory/cycles.md)) |
| `override` | `to_string`, `hash_code` | Replaces the object-protocol method; must also be `public` (see [Objects](objects.md)) |

## Special members

| Member | Declared as | Used by |
|--------|-------------|---------|
| Constructor | `constructor(...)`, or header parameters | `Type(...)` |
| Destructor | `del()` | Runs right before a class instance is destroyed |
| Getter / setter | `get name(): T` / `set name(value: T)` | `obj.name` / `obj.name = v` |
| Indexer | `fun this[index: int]: T` / `fun this[index: int] = value: T` | `obj[i]` / `obj[i] = v` |
| Enumerator | `fun iterator()` returning an object with `fun next(): Option<T>` | `for (let x in obj)` |
| Operators | `fun operator +(other: T): T` and friends | `a + b` (see [Operator overloading](../abstraction/operator-overloading.md)) |

## Classes are reference types

A `class` lives on the heap, and assigning or passing it shares that object. Parameters that take ownership follow [ownership](../memory/ownership.md):

```dream
class Point(x: int, y: int);

let p1 = Point(3, 4);
let p2 = p1; // shares the same object
p2.x = 10;
System.println(p1.x); // 10
```

You do not free a class yourself. A `del()` destructor runs right before the object is destroyed — see [Memory Management](../memory/index.md).

Header parameters become **public** fields and a `constructor`. Write `internal` to narrow one, and extra constructors in the body are also named `constructor`.

Call sites stay `Point(3, 4)`.

### Overloaded constructors

A class or struct may declare more than one `constructor`, resolved by argument count/types exactly like an overloaded function or method (see [Language Invariants](../rules/invariants.md#overloading)):

```dream
class Point {
    public x: int;
    public y: int;

    public constructor() {
        this.x = 0;
        this.y = 0;
    }

    public constructor(x: int, y: int) {
        this.x = x;
        this.y = y;
    }

    public constructor(both: int) {
        this.x = both;
        this.y = both;
    }
}

let a = Point(); // (0, 0)
let b = Point(3, 4); // (3, 4)
let c = Point(5); // (5, 5)
```

## Structs are value types

A `struct` is stored inline (on the stack, inside an array, or inside another object), and every assignment or argument pass makes an independent **copy** (not a last-use [move](../memory/ownership.md)):

```dream
struct Vec2(x: int, y: int);

let v1 = Vec2(3, 4);
let v2 = v1; // full copy
v2.x = 10;
System.println(v1.x); // 3 (unaffected)
```

Structs need no heap allocation and no automatic reference tracking, so a struct held by value is never absent and cannot recursively contain itself by value. Use `Option<S>` when a struct value may be empty.

### `ref struct`: a stack-only value type

`ref struct` is a `struct` that must stay in the current stack frame. Use it for a view such as `Span<T>` (see [Arrays](arrays.md)):

```dream
ref struct Pair {
    public a: int;
    public b: int;

    public constructor(a: int, b: int) {
        this.a = a;
        this.b = b;
    }
}
```

A `ref struct` value behaves exactly like an ordinary `struct` as a local variable, a function parameter, or a function return value (inline storage, copy semantics, zero heap allocation). What's rejected:

- **Storing it in a field** of a `class` or an ordinary `struct` — that would keep it alive past the frame that created it. A field of another `ref struct` is fine, since the outer value is bound by the same rules (`StringSplit` holds a `ReadOnlySpan<char>` this way).
- **Using it as a generic type argument** of a type or a function (`List<Pair>`, `Option<Pair>`, `identity<Pair>(p)`, ...) — generic code may store a `T` anywhere, including on the heap.
- **Using it as an array element type** (`Pair[]`, `[p, q]`) — arrays are heap-allocated.
- **Capturing it in a lambda** — a capturing lambda's environment is a heap-allocated cell.
- **Using it as a parameter of an `async` function**, or **keeping a local of that type in scope at an `await`** — an async function's locals live in its heap-allocated coroutine state across a suspend point. Move the span work into a separate non-async function and `await` around it.

`ref` may only precede `struct`. `ref class` is a parse error.

### When to use which

- Use a **`struct`** for small, copyable bundles with value identity — points, vectors, colors, ranges.
- Use a **`class`** when an instance has a lifetime and identity that should be *shared* rather than copied — graph nodes, file handles, services.

## Shared features

Both classes and structs support all of the following.

### Visibility

Members are private to the type unless you mark them `internal` or `public`. `static` does not change visibility.

Separately, the type itself is **file-private by default** and needs `public` (or `internal`) to be used from another file. Full rules: [Imports > Visibility](../rules/imports.md#visibility).

```dream
module utils.math;

public class Counter {
    count: int; // private: only Counter's methods
    internal step: int; // visible anywhere in module utils.math

    public fun value(): int {
        return this.count;
    }
}
```

A field may also carry `weak` or `unowned` (combinable with visibility in any order) so the field does not extend its target's lifetime — see [Weak and unowned references](../memory/cycles.md#weak-and-unowned-references).

### Methods

Declare methods with `fun`; each receives an implicit `this`:

```dream
class Counter {
    count: int;

    public fun increment(): void {
        this.count = this.count + 1;
    }
}
```

### Properties

Computed properties use `get` / `set` accessors. Reading `obj.name` calls the getter; assigning `obj.name = v` calls the setter. They take the same visibility modifiers as methods (`public` / `internal` / private) and may be `static`:

```dream
class Temperature {
    celsius: float;

    public get fahrenheit(): float {
        return this.celsius * 9.0f / 5.0f + 32.0f;
    }

    public set fahrenheit(value: float) {
        this.celsius = (value - 32.0f) * 5.0f / 9.0f;
    }
}

class App {
    public static get version(): int {
        return 1;
    }
}
```

A getter-only property is fine; a setter without a getter is allowed but unusual. These are distinct from bracket indexers (`fun this[...]`) below.

### Indexers and enumerators

Opt into `obj[i]` / `obj[i] = v` with indexers. Opt into `for let x in obj` with methods named `iterator` (zero args, returning an enumerator object) and `next` (zero args, returning `Option<T>`):

```dream
class Grid {
    public fun this[index: int]: int {
        ...
    }

    public fun this[index: int] = value: int {
        ...
    }

    public fun iterator(): GridIter {
        ...
    }
}

class GridIter {
    public fun next(): Option<int> {
        ...
    }
}
```

A method named `get` / `set` is ordinary and does **not** enable bracket sugar. `iterator` / `next` are the enumerator protocol when they have the shapes above.

## Advanced: sealed types

Prefix a `class`, `struct`, or `enum` with `sealed` to forbid `extend` blocks from adding methods, locking the method surface to what the type declares:

```dream
sealed class Token {
    public kind: int;
}

// error: Cannot extend sealed type 'Token'
extend Token {
    public fun describe(): string {
        return "token";
    }
}
```

`sealed` combines with `public` in either order (`public sealed class ...`). It only blocks user `extend` blocks — a sealed type may still implement interfaces (including their defaults) and derive `@json`.

## Advanced: static classes

A `static class` is a namespace of static members, not a value type. Use it for helpers like `System` and `Math`:

```dream
public static class Util {
    public static fun twice(n: int): int {
        return n * 2;
    }
}

System.println(Util.twice(3)); // 6
```

A static class cannot have instance fields, instance methods, constructors, or an `implements` clause. Members must be marked `static`. It cannot be instantiated (`Util()`), used as a type (`let x: Util`, `List<Util>`), or grow instance methods via `extend`.

A later `extend Util { public static fun … }` may still add more static helpers (stdlib splits `Math` this way). `static` cannot modify `struct`, `enum`, or `interface`.

```dream
// error: cannot instantiate static class 'Util'
let u = Util();
```

## Advanced: `shared` classes

Prefix a `class` with `shared` to use it from more than one task. Each instance can be locked.

A type is **`shared`** (and may be captured by a [`Task`](../concurrency/tasks.md) body, or stored in a `shared class`) when it is unmanaged / blittable, `string`, a value struct whose fields are all `shared`, or a `shared class`. Constrain generics with `T : shared`.

```dream
shared class Counter {
    public value: int;
    public label: string; // legal — string is shared

    public constructor() {
        this.value = 0;
        this.label = "";
    }

    public fun increment(): void {
        lock (this) {
            this.value = this.value + 1;
        }
    }
}
```

Every field of a `shared class` must be `shared`, or a managed reference whose reachable values join this object: another class, an array (`int[]`, `Job[]`), `List<T>`, or a union such as `Option<Plain>`. Those fields use the same `lock` as every other field.

A value struct that embeds a non-shared class, or a generic argument that could, is rejected.

```dream
class Plain {
    public x: int;
}

shared class Holder {
    // legal: managed heap references — the graph below `h` is part of the shared object
    public p: Option<Plain>;

    public xs: int[];

    public constructor() {
        this.p = Option.None;
        this.xs = [];
    }
}

struct Wrap {
    public p: Plain; // value struct embedding a non-shared class
}

shared class Bad {
    // error: 'Wrap' is a value struct containing a non-shared reference
    public w: Wrap;
}
```

`shared struct` is not allowed — value structs become `shared` automatically when their fields are. Wrap a reference graph in a `shared class` if it needs to be shared by pointer.

`lock (x)` requires a `shared class`, not every `shared` type. See [`lock (obj) { ... }`](../concurrency/sharing.md#sharing-state-safely).

## Advanced: boxing a struct

When a struct is used where a reference is expected, including an interface or `object` variable, Dream copies it onto the heap. `Option<Vec2>` stays an `Option` of the value.
