# Enums & Unions

`enum` covers two related ideas: a simple enum is a set of named integer constants, and a *discriminated union* is an enum whose variants carry typed data. You take unions apart with a pattern-matching `switch`.

## Enums

A simple `enum` defines named integer constants. Members number from `0`; an explicit value shifts the ones that follow:

```dream
enum Color { Red, Green, Blue }          // 0, 1, 2
enum Status { Active = 10, Inactive }    // 10, 11
```

Access a member with `Enum.Member`. Enum values are integers at runtime, so they interoperate with `int` and work as `switch` subjects and labels:

```dream
let c: Color = Color.Green;
System.println(c);              // 1
System.println(c.to_string());  // Green
```

Simple enums also take bitwise `&`, `|`, `^`, and prefix `~` (same as `int`). Combine flag variants with `|`; the result stays the enum type. Shifts (`<<` / `>>`) stay integer-only.

```dream
enum Flags { None = 0, Read = 1, Write = 2, Exec = 4 }

let rw: Flags = Flags.Read | Flags.Write;
let can_read = (rw & Flags.Read) != Flags.None;  // true
```

Discriminated unions are heap values, not integers — bitwise operators do not apply to them.

## Discriminated unions

When **any** variant carries a payload `(...)`, the whole `enum` becomes a discriminated union. A value is exactly one variant, and each variant holds its own typed data:

```dream
enum Shape {
    Circle(float),                   // positional payload
    Rect(width: float, height: float),
    Empty,                           // a unit variant carries no data
}

let s = Shape.Circle(2.0);
let e = Shape.Empty;
```

A one-field payload may be positional (`Circle(float)`) or named (`Full(value: int)`). Construction accepts either order that matches the declaration: positional `Full(7)` always works; `Full(value: 7)` works when the field is named.

### Pattern-matching switch

The pattern form of `switch` runs the first arm whose pattern fits and binds the payload. The variant qualifier is optional inside the arms because the subject type is known. It works in both expression and statement position:

```dream
// expression position: yields a value
let area = switch (s) {
    Circle(r)  => 3.14 * r * r,
    Rect(w, h) => w * h,
    Empty      => 0.0,
};

// statement position: arms may be blocks
switch (s) {
    Circle(r)  => { System.println(r); }
    Rect(w, h) => System.println(w * h),
    Empty      => System.println("empty"),
}
```

A pattern `switch` must be **exhaustive**. Cover every variant, or add a catch-all `_` (or binding) pattern:

```dream
switch (s) {
    Circle(r) => System.println("Circle"),
    _         => System.println("Other"),
}
```

### Advanced patterns

Patterns **nest** — a payload can be matched against a variant. Exhaustiveness is checked recursively, so covering every inner case covers the outer variant with no `_`:

```dream
enum Inner { A(v: int), B }
enum Outer { Wrap(inner: Inner), Bare }

switch (o) {
    Wrap(A(n)) => n,
    Wrap(B)    => -1,   // Wrap(A) + Wrap(B) together cover Wrap
    Bare       => 0,
}
```

Guards (`if <bool>`) narrow an arm further:

```dream
switch (opt) {
    Some(n) if n > 10 => System.println("big"),
    Some(n)           => System.println(n),
    None              => System.println("none"),
}
```

### Or-patterns and range patterns

An arm's pattern may be several alternatives separated by `|` — the arm runs if the subject matches any of them. Every alternative must be **binding-free** (a literal, a range, `_`, or a payload-free variant); an alternative that would bind a variable (`Circle(r)`, a bare name) is rejected, since which alternative matched isn't visible to pick a binding from:

```dream
switch (c) {
    'a' | 'e' | 'i' | 'o' | 'u' => System.println("vowel"),
    _                           => System.println("consonant"),
}

switch (shape) {
    Square | Triangle | Empty => System.println("no curves"),
    Circle(_)                 => System.println("curved"),
}
```

A literal pattern over an ordered scalar subject (`int` / `long` / `uint` / `ulong` / `byte` / `char` / `float` / `double`) can be an inclusive range, `lo..hi`:

```dream
fun grade(score: int): string {
    return switch (score) {
        90..100 => "A",
        80..89  => "B",
        70..79  => "C",
        _       => "F",
    };
}
```

Both compose with exhaustiveness checking the same way a single pattern does — `Square | Triangle | Empty` together with `Circle(_)` covers every variant with no `_` needed, for example.

### Generic unions

Unions may be generic; the concrete type is inferred from constructor arguments, or supplied by annotation. Add methods with an `extend` block:

```dream
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }

let o  = Option.Some(42);         // inferred Option<int>
let n: Option<int> = Option.None; // annotation needed for the unit variant
```

### Value unions

Unions are heap values by default.

A union becomes a **value union** when every payload is a value (`int`, `bool`, `float`, a `struct`, and so on):

- It is stored inline and copied on assignment.
- It does not allocate on the heap.
- This is decided for each concrete type. `Option<int>` is a value union. `Option<string>` holds a reference payload, so it is a [niche union](#niche-unions) instead.

#### `enum struct`: a value union, plus a reference-payload relaxation

`enum struct` is a value union you ask for, the same split as `class` vs `struct`.

- If it cannot be stored inline, that is an error. Dream does not silently put it on the heap.
- Adding a payload that refers to the same union is an error, and the message names that field.
- Reference payloads are allowed: `string`, `class`, arrays, and so on. Each is stored as a pointer inside the value, the same way a `struct` field holds one.
- A union cannot contain itself inline. That value would have no finite size.

```dream
enum struct Outcome {
    Success(code: int),
    Failure(code: int, reason: string), // reference fields are fine
}

enum struct Either {
    Left(string),
    Right(string), // multiple reference fields across variants: also fine
}
```

Write `enum struct` when you want this. Dream does not do it automatically when several variants hold references.

A union with exactly one reference payload is handled on its own. See the next section.

`enum struct` on a simple enum (no payloads) is an error. Use `enum`.

#### Niche unions

Some `Option`-like unions store "none" without an extra flag.

That happens when there are exactly two variants: one empty, and one with a single reference payload (`Option<TreeNode>`, `Option<string>`). `None` and `Some(x)` share that slot.

- Pattern matching works the same.
- A `weak` field typed `Option<Class>` becomes `None` when the object is gone.
- This is chosen per concrete type. `Option<int>` stays a value union. Unions with more than two variants, or with extra payloads, stay on the heap.

### JSON with `@json`

Mark a union `@json` to derive `to_json` / `from_json`. Each value serializes to an object tagged with a `"type"` key naming the active variant:

```dream
@json
enum Shape { Circle(int), Rect(width: int, height: int), Empty }

let text = Json.serialize(Shape.Circle(7));   // {"type":"Circle","_0":7}
```

Named payload fields serialize under their names (`Rect` gives `"width"` / `"height"`); positional ones use `_0`, `_1`, … in order.
