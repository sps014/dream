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
System.println(15.clamp(0, 10));              // 10
System.println((-5).abs());                   // 5
let n = int.parse("42").unwrap_or(0);  // 42
```

### Integer overflow

Integer arithmetic **wraps** by default: an operation whose mathematical result does not fit its type wraps modulo the type's bit width (two's complement), in debug and release builds alike. Nothing is silently promoted to a wider type.

- A binary op's result type is its **left operand's** type — `byte + byte` stays `byte`, so `250b + 10b` is `4b`.
- `+`, `-`, `*`, and unary `-` wrap: `2147483647 + 1` is `-2147483648`, and `uint` `0u - 1u` is `4294967295u`.
- `/` and `%` [panic](panics.md) on a zero divisor. `-2147483648 / -1` (the minimum signed value divided by `-1`) wraps to `-2147483648`, and the matching `%` is `0`.
- Shift counts are masked to the type's bit width: `1 << 33` on an `int` is `1 << 1`. Bits shifted out of the value are discarded: `1 << 31` is `-2147483648`.

```dream
let i: int = 2147483647;   // the largest int
let j = i + 1;              // -2147483648
```

Wrap a `checked { }` block around code where an overflow is a bug you want to catch (sizes, offsets, money). Inside it, every integer op lexically in the block panics instead of wrapping:

- `+`, `-`, `*`, and unary `-` panic when the result is outside the type.
- `/` and `%` also panic on the minimum signed value divided by `-1`.
- `<<` and `>>` panic when the shift count is negative or at least the type's bit width.

`unchecked { }` restores wrapping inside a `checked` region:

```dream
let count = 100000;
let size = 100000;
let h = 2166136261u;
checked {
    let total = count * size;        // panic: attempt to multiply with overflow
    unchecked {
        h = (h ^ 97u) * 16777619u;   // FNV-1a step: wraps at 32 bits
    }
}
```

The mode is lexical: it applies to the statements written inside the block (including lambdas written there), not to functions they call. `checked` and `unchecked` are only keywords directly before `{`, so they stay usable as identifiers. GPU shader code always wraps, and rejects `checked` blocks.

Inside `checked` blocks, the optimizer removes checks it can prove never fire — a loop counter bounded by `i < n`, a masked `x & 255`, an array length.

`Type.parse(str)` reports out-of-range text as `Err(ParseError)` rather than panicking.

## Floating point

IEEE 754, in two widths:

- `float` — 32-bit (`3.14f`). An unsuffixed `3.14` is also `float` unless the expected type is `double`.
- `double` — 64-bit. Use `3.14d` for an always-double literal, or a bare `3.14` / `0` when the expected type is `double`.

Common methods:

- `.abs()` — absolute value.
- `.min(other)` / `.max(other)`.
- `double.parse(str)` — static; parses a string into a `double`, returning `Result<double, ParseError>`.

```dream
System.println((3.14f).abs());                    // 3.14
System.println((1.5f).min(2.0f));                 // 1.5
let d = double.parse("2.5").unwrap_or(0.0d);
```

## Booleans

`bool` is `true` or `false`.

- `.to_int()` — `1` for `true`, `0` for `false`.
- `bool.parse(str)` — static; accepts exactly `"true"` or `"false"` (case-sensitive), returning `Result<bool, ParseError>`.

```dream
System.println(true.to_int());   // 1
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
System.println('A'.is_alpha());   // true
System.println('A'.to_lower());   // 'a'
let s = 'H'.as_string();   // "H"
let c = char.parse("é").unwrap_or('?');  // 'é'
```
