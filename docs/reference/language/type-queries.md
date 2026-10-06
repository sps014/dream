# Inspect names, types, and sizes

Use these expressions when you need a name, type description, or storage size.

[Back to overview](operators.md)

## `sizeof`, `nameof`, and `typeof`

None of these names is a reserved keyword — each takes on its meaning only immediately before `(`,
so you can still declare a variable or function called `typeof`. Written as a call, they are
meta forms:

```dream
struct Point { public x: int; public y: int; }

let bytes: int = sizeof(Point);     // 8 — byte size of the struct
let ptr_w: int = sizeof(string);    // 4 on wasm32, 8 on 64-bit native targets
let name: string = nameof(Point.x); // "x" — last path segment; operand is not evaluated
let kind: string = typeof(bytes);   // "int"
```

- **`sizeof(T)`** yields an `int`:
  - primitives and value `struct`s → their storage size in bytes
  - class instances, arrays, `string`, and other heap refs → the target's pointer width
  - nested value structs, tuples, and value unions use the same target layout as code generation
  - The result is a compile-time constant.
- **`nameof(a.b.c)`** yields a `string` of the last identifier in a dotted path. The path is not
  type-checked or evaluated (you can write `nameof(future_api)`).
- **`typeof(expr)`** yields a `string` naming the operand's concrete type. See below.

### `typeof`

`typeof` answers "what did I actually get?", which is most useful when a value has been widened to
`object` or to an interface:

```dream
let d: Map<string, object> = { "ok": "ko" };
System.println(typeof(d));        // "Map<string, object>" — the source spelling, not a mangled name

let boxed: object = 5;
System.println(typeof(boxed));    // "int" — the payload type, not "object"

let shape: Shape = Circle();
System.println(typeof(shape));    // "Circle" — the implementing class
```

There are two resolution paths, and which one applies is decided at compile time:

| Operand's static type | How it resolves |
|---|---|
| `object`, or an interface | Reads the value's type tag and returns a string |
| everything else | Folds to a compile-time string constant |

The folded path costs nothing at runtime and, like `nameof`, **does not evaluate its operand** —
`typeof(f())` will not call `f`. The runtime path does not allocate.

Three type tags are shared across every instantiation of their shape, so `typeof` reports a
coarse name for them when the static type was erased: an array reports `"array"`, a lambda or
function value reports `"function"`, and a `Future<T>` reports `"future"`. A statically typed
operand of those shapes still reports precisely (`typeof(nums)` on an `int[]` is `"int[]"`). A null
reference reports `"null"`.
