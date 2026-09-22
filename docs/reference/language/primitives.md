# Primitives

**Package:** `system.primitives` (bootstrap — no import required)

Primitive types are the built-in scalars: integers, floats, booleans, and characters. Their methods ship in the always-on prelude, so you can call them anywhere without an import. For the full type list and literal suffixes, see [Types](types.md).

## Integers

Signed and unsigned, in several widths:

- `int` — 32-bit signed (the default for integer literals: `42`).
- `uint` — 32-bit unsigned (`42u`).
- `long` — 64-bit signed (`42L`).
- `ulong` — 64-bit unsigned (`42uL`).
- `byte` — 8-bit unsigned (`255b`).

Common methods:

- `.min(other)` / `.max(other)` — the smaller / larger of two values.
- `.clamp(lo, hi)` — constrain to the inclusive range `[lo, hi]`.
- `.abs()` — absolute value (signed types only).
- `.signum()` — `-1`, `0`, or `1` by sign (signed types only).
- `Type.parse(str)` — static; parses a string into that integer type, returning `Result<Type, ParseError>`.

```dream
println(15.clamp(0, 10));              // 10
println((-5).abs());                   // 5
let n = int.parse("42").unwrap_or(0);  // 42
```

### Integer overflow

Every integer primitive **wraps** on overflow: two's-complement modulo its own bit width, with no trap and no promotion to a wider type.

- `int` / `uint` wrap at 32 bits, `long` / `ulong` at 64 bits, `byte` at 8 bits (`+`, `-`, `*`, `<<` all wrap; `byte` results stay in `[0, 255]`).
- A binary op's result type is its **left operand's** type — `byte + byte` stays `byte`.
- `/` and `%` by zero panic rather than wrapping.

```dream
let i: int = 2147483647;   // int.max
let j = i + 1;              // wraps to -2147483648, not a panic or a wider type

let b: byte = 250b;
let b2 = b + 10b;           // wraps to 4 (260 mod 256), stays byte
```

There is no `checked` or saturating arithmetic mode. Use `.min` / `.max` / `.clamp()` above, or check operands before an operation, if you need to guard against wraparound explicitly.

## Floating point

IEEE 754, in two widths:

- `float` — 32-bit (`3.14f`). An unsuffixed `3.14` is also `float` unless the expected type is `double`.
- `double` — 64-bit. Use `3.14d` for an always-double literal, or a bare `3.14` / `0` when the expected type is `double`.

Common methods:

- `.abs()` — absolute value.
- `.min(other)` / `.max(other)`.
- `double.parse(str)` — static; parses a string into a `double`, returning `Result<double, ParseError>`.

```dream
println((3.14f).abs());                    // 3.14
println((1.5f).min(2.0f));                 // 1.5
let d = double.parse("2.5").unwrap_or(0.0d);
```

## Booleans

`bool` is `true` or `false`.

- `.to_int()` — `1` for `true`, `0` for `false`.
- `bool.parse(str)` — static; accepts exactly `"true"` or `"false"` (case-sensitive), returning `Result<bool, ParseError>`.

```dream
println(true.to_int());   // 1
let b = bool.parse("true").unwrap_or(false);   // true
let bad = bool.parse("True").unwrap_or(false); // false — not exact "true"
```

## Characters

`char` is a single character (one Unicode scalar value). Write literals in single quotes: `'A'`, `'\n'`, `'é'`.

- `.is_digit()` / `.is_alpha()` / `.is_whitespace()` — classify the character (ASCII rules).
- `.to_lower()` / `.to_upper()` — ASCII case conversion.
- `.to_int()` — the numeric code point.
- `.as_string()` — a new single-character string.
- `char.parse(str)` — static; requires exactly one Unicode scalar in `str`; returns `Result<char, ParseError>`.

```dream
println('A'.is_alpha());   // true
println('A'.to_lower());   // 'a'
let s = 'H'.as_string();   // "H"
let c = char.parse("é").unwrap_or('?');  // 'é'
```
