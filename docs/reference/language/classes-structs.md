# Classes & Structs

Classes and structs both group related data with fields, constructors, and methods. They share every feature — the one difference is **how they are stored and copied**: a `class` is a reference type, a `struct` is a value type.

## Classes are reference types

A `class` lives on the heap, and a variable holds a *reference* to it. Assigning or passing a class shares the same object. Heap values also follow [ownership](ownership.md) (sink parameters, last-use move):

```dream
class Point(public x: int, public y: int);

let p1 = Point(3, 4);
let p2 = p1;    // shares the same object
p2.x = 10;
println(p1.x);  // 10
```

Classes are managed by automatic reference counting (ARC) — no manual frees. Define a `del()` destructor and it runs right before the object is destroyed. See [Memory Management](memory.md).

Header parameters become fields and a synthesized `constructor`. Extra overloads in the body are also named `constructor`. Call sites stay `Point(3, 4)`.

### Overloaded constructors

A class or struct may declare more than one `constructor`, resolved by argument count/types exactly like an overloaded function or method (see [Language Invariants](invariants.md#overloading)):

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

let a = Point();      // (0, 0)
let b = Point(3, 4);  // (3, 4)
let c = Point(5);     // (5, 5)
```

## Structs are value types

A `struct` is stored inline (on the stack, inside an array, or inside another object), and every assignment or argument pass makes an independent **copy** (not a last-use [move](ownership.md)):

```dream
struct Vec2(public x: int, public y: int);

let v1 = Vec2(3, 4);
let v2 = v1;    // full copy
v2.x = 10;
println(v1.x);  // 3 (unaffected)
```

Structs need no heap allocation and no retain/release, so a struct held by value is never absent and cannot recursively contain itself by value. Use `Option<S>` when a struct slot may be empty.

### `ref struct`: a stack-only value type

`ref struct` is a `struct` with one additional restriction: the compiler rejects any use that would let an instance escape the current stack frame. It exists for types like `Span<T>` (see [Arrays](arrays.md)) that wrap a raw address into memory whose lifetime the compiler cannot otherwise track:

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

- **Storing it in a field** of any `class` or `struct` — that would keep it alive past the frame that created it.
- **Using it as a generic type argument** (`List<Pair>`, `Option<Pair>`, ...) — a container's backing storage is heap-allocated.
- **Capturing it in a lambda** — a capturing lambda's environment is a heap-allocated cell.
- **Using it as a parameter of an `async` function** — an `await` suspend point spills the coroutine's live locals into heap-allocated state.

`ref` may only precede `struct`, never `class` (a `class` is already a heap-allocated reference type, so "stack-only class" is meaningless and rejected at parse time).

### When to use which

- Use a **`struct`** for small, copyable bundles with value identity — points, vectors, colors, ranges.
- Use a **`class`** when an instance has a lifetime and identity that should be *shared* rather than copied — graph nodes, file handles, services.

## Shared features

Both classes and structs support all of the following.

### Visibility

Members (fields, methods, static members, accessors, constructors) are **class-private by
default** — reachable only from the type's own methods, regardless of file. Mark a member
`internal` (module-wide) or `public` (everywhere the type is reachable) to expose it. `static`
never implies visibility. Separately, the type itself is **file-private by default** and needs
`public` (or `internal`) to be used from another file. Full rules: [Imports > Visibility](imports.md#visibility).

```dream
module utils.math;

public class Counter {
    count: int;                 // private: only Counter's methods
    internal step: int;         // visible anywhere in module utils.math
    public fun value(): int { return this.count; }
}
```

A field may also carry `weak` or `unowned` (combinable with visibility in any order) to opt a
strong-reference-cycle-prone field out of the compiler's cycle check — see
[Memory > Reference cycles](memory.md#advanced-reference-cycles).

### Methods

Declare methods with `fun`; each receives an implicit `this`:

```dream
class Counter {
    count: int;
    public fun increment(): void { this.count = this.count + 1; }
}
```

### Properties

Computed properties use TypeScript-style `get` / `set` accessors. Reading `obj.name` calls the
getter; assigning `obj.name = v` calls the setter. They take the same visibility modifiers as
methods (`public` / `internal` / private) and may be `static`:

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
    public static get version(): int { return 1; }
}
```

A getter-only property is fine; a setter without a getter is allowed but unusual. These are
distinct from bracket indexers (`fun this[...]`) below.

### Indexers and enumerators

Opt into `obj[i]` / `obj[i] = v` with C#-style indexers. Opt into `for let x in obj` with methods named `iterator` (zero args, returning an enumerator object) and `next` (zero args, returning `Option<T>`):

```dream
class Grid {
    public fun this[index: int]: int { ... }
    public fun this[index: int] = value: int { ... }
    public fun iterator(): GridIter { ... }
}

class GridIter {
    public fun next(): Option<int> { ... }
}
```

A method named `get`/`set` is ordinary and does **not** enable bracket sugar. `iterator` / `next` are the enumerator protocol when they have the shapes above.

## Advanced: sealed types

Prefix a `class`, `struct`, or `enum` with `sealed` to forbid `extend` blocks from adding methods, locking the method surface to what the type declares:

```dream
sealed class Token { public kind: int; }

// error: Cannot extend sealed type 'Token'
extend Token { public fun describe(): string { return "token"; } }
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

System.println(Util.twice(3));   // 6
```

A static class cannot have instance fields, instance methods, constructors, or an `implements` clause. Members must be marked `static`. It cannot be instantiated (`Util()`), used as a type (`let x: Util`, `List<Util>`), or grow instance methods via `extend`. A later `extend Util { public static fun … }` may still add more static helpers (stdlib splits `GpuMath` this way). `static` cannot modify `struct`, `enum`, or `interface`.

```dream
// error: cannot instantiate static class 'Util'
let u = Util();
```

## Advanced: `shared` classes

Prefix a `class` with `shared` to make it a **shared** reference type — Dream's analogue of Swift `Sendable` for classes. A `shared class` pays two costs, and only when opted in:

- **Atomic refcounting.** Retain/release use atomic instructions instead of the ordinary fast path, since a shared instance's refcount can be touched from more than one thread.
- **An extra header word** reserved for a reentrant lock, used by [`lock (obj) { ... }`](webworkers.md#sharing-state-safely) and the instance's own implicit locking.

A type is **`shared`** (and may be captured by a [`WebWorker`](webworkers.md) body, or stored in a `shared class`) when it is unmanaged / blittable, `string`, a value struct whose fields are all `shared`, or a `shared class`. Constrain generics with `T : shared`.

```dream
shared class Counter {
    public value: int;
    public label: string;   // legal — string is shared
    public constructor() { this.value = 0; this.label = ""; }

    public fun increment(): void {
        lock (this) {
            this.value = this.value + 1;
        }
    }
}
```

**The closed-graph field rule:** every field of a `shared class` must be either `shared`, or a **managed heap reference** — another ordinary class, an array (`int[]`, `Job[]`), `List<T>`, a union like `Option<Plain>` — whose whole reachable graph then joins this object's shared region. Such fields use the same `lock` discipline as every other field; there are no per-field access rules. What is rejected is anything that would smuggle an *untracked* reference in: a value struct embedding a non-shared class field, or a generic argument that could be one.

```dream
class Plain { public x: int; }

shared class Holder {
    // legal: managed heap references — the graph below `h` is part of the shared object
    public p: Option<Plain>;
    public xs: int[];

    fun new() { this.p = Option.None; this.xs = []; }
}

struct Wrap {
    public p: Plain;   // value struct embedding a non-shared class
}

shared class Bad {
    // error: 'Wrap' is a value struct containing a non-shared reference
    public w: Wrap;
}
```

`shared struct` is not allowed — value structs become `shared` automatically when their fields are. Wrap a reference graph in a `shared class` if it needs to be shared by pointer. `lock (x)` still requires a `shared class` (a lock word), not every `shared` type.

## Advanced: boxing a struct

When a struct is used where a reference is expected, it is **boxed** into a heap copy. An optional struct (`Option<Vec2>`) is an ordinary `Option` over the value type — and assigning a struct to a bare interface or `object` variable boxes it for dynamic dispatch.
