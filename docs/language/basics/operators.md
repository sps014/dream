---
title: Operators
summary: "Arithmetic, comparison, logical, and assignment operators."
icon: material/plus-minus-variant
status: stable
targets: [native, web, node]
---

# Operators

Operators calculate, compare, or update values. Later sections cover custom operators and evaluation order.

!!! glance "At a glance"

    ```dream
    let total = price * quantity + shipping;
    let ready = total > 0 && paid;
    count += 1;
    ```

    - Arithmetic, comparison, and logical operators work like most C-family languages.
    - Compound assignments such as `+=` update a variable in place.

## Explore this topic

- [Define operators for your types](../abstraction/operator-overloading.md)
- [Inspect names, types, and sizes](type-queries.md)

## Arithmetic

| Operator | Meaning | Types |
|----------|---------|-------|
| `+` | Addition / string concat | `int`, `float`, `double`, `string` |
| `-` | Subtraction | `int`, `float`, `double` |
| `*` | Multiplication | `int`, `float`, `double` |
| `/` | Division | `int`, `float`, `double` |
| `%` | Remainder | `int`, `float` |

Both operands must be the same type. Cast one if they differ:

```dream
let x = 7 / (float)2; // 3.5
```

Prefix `-` negates a number: `let neg = -x;`.

Integer arithmetic (`+`, `-`, `*`, `/`, `%`, `<<`, `>>`, and unary `-`) **wraps** at the type's
bit width on overflow rather than widening; `/` and `%` panic on a zero divisor. Inside a
`checked { }` block the same operators panic on overflow instead — see
[Primitives § Integer overflow](../types/primitives.md#integer-overflow) for the full policy.

## String concatenation

When either side of `+` is a `string`, the other side is converted through its [`to_string`](../../library/core/builtins.md). A simple enum renders its variant *name*, not the number:

```dream
let msg = "Hello, " + name + "!";
let line = "color = " + Color.Green; // "color = Green"
```

## String interpolation

Prefix a string with `$` and wrap expressions in `{ ... }`. Each hole is evaluated and converted to a string, just like `+`:

```dream
let name = "Ada";
let count = 3;
let msg = $"{name} has {count + 1} items"; // "Ada has 4 items"
```

Interpolation expands to concatenation, so the above equals `"" + name + " has " + (count + 1) + " items"`.

Double a brace to write it literally — `{{` produces `{`, `}}` produces `}`:

```dream
let x = 5;
let s = $"{{literal}} and {x}"; // "{literal} and 5"
```

A hole may contain a string literal (`$"x is {"hi"}"`). Escape a quote in the outer interpolation with `\"`.

## Comparison

All comparisons return `bool`.

| Operator | Meaning |
|----------|---------|
| `==` | Equal |
| `!=` | Not equal |
| `<` `<=` `>` `>=` | Ordering |

String `==` and `!=` compare **contents**, not addresses.

## Logical

`&&` (and), `||` (or), and `!` (not) operate on `bool`. `&&` and `||` **short-circuit**: the right operand runs only when it can still change the result.

```dream
false && boom(); // boom() never runs
true || boom(); // boom() never runs
```

## Bitwise

`&`, `|`, `^`, `<<`, `>>`, and prefix `~` work on `int`, `uint`, `long`, `ulong`, and `byte`. Both operands of a binary operator must be the same type.

- `&`, `|`, `^`, and `~` also work on simple enums and keep the enum type (`Flags.Read | Flags.Write`). Shifts stay on integers.
- `>>` sign-extends `int` and `long`, and zero-fills `uint`, `ulong`, and `byte`.

```dream
let flags: uint = 6u; // 0b0110
let masked = flags & 4u; // 4u   (0b0100)
let shifted: byte = 200b >> 2b; // 50b, zero-filled
let inverted = ~5; // -6 (two's complement)
let inverted_b: byte = ~5b; // 250b (wraps within byte's 0..255 range)
```

Like arithmetic, `~` and the binary bitwise ops on `byte` wrap their result into `byte`'s `0..255`
range — see [Primitives § Integer overflow](../types/primitives.md#integer-overflow).

## Null-coalescing and ternary

`a ?? b` yields the value inside `a` when `a` is `Option.Some(...)`, otherwise `b`. The left side
is an `Option<T>` and the result is `T` (equivalent to `a.unwrap_or(b)`):

```dream
let name: Option<string> = lookup();
let display: string = name ?? "anonymous";
```

`cond ? a : b` picks `a` when `cond` is true, else `b`. Both branches must share a type:

```dream
let label = score >= 60 ? "pass" : "fail";
```

## Try-propagation

`expr?` unwraps a `Result<T, E>`/`Option<T>`, or `return`s the failure/absence variant from the
enclosing function immediately. See [Option & Result](../../library/core/option-result.md#try-propagation)
for the full rules.

```dream
fun quarter(n: int): Result<int, string> {
    let h = half(n)?;
    return Result.Ok(half(h)?);
}
```

Postfix `?` wins over the ternary unless a matching `:` follows at the same nesting depth. `half(n)? + 1` is try-propagation, and `cond ? a : b` is still the ternary.

## Assignment

`=` writes to a variable, array element, or field:

```dream
x = 10;
arr[0] = 99;
point.x = 3;
```

Compound forms update in place, and `++`/`--` step by one (prefix or postfix; as statements or
expressions). Postfix yields the old value; prefix yields the new:

```dream
total += 5; // total = total + 5
count++;
++i;
let prev = j++;
let next =++j;
for (let k = 0; k < n; k++) {}
```

Discard a value without binding a name using `_` (like a pattern wildcard):

```dream
let _ = sideEffect();
let (_, y) = pair;
let _ = fetch().await;
```

Unread `let` and `const` locals warn, and the program still compiles. Use `_` to ignore a value.

A name such as `_guard` is a real binding. It lasts to the end of the scope and is never reported unused, and `nameof(local)` counts as a use.

Any expression can be used as a statement (`expr;`); the result is evaluated and dropped.

## Range expressions

`a..b` produces a `Range` with `int` bounds and an excluded end. It steps toward `b`, and equal bounds are empty. See [Ranges](../types/ranges.md).

## Precedence

Higher rows bind tighter; use parentheses when in doubt.

| Precedence | Operators |
|------------|-----------|
| postfix | `?` (try-propagation) |
| unary | unary `-`, `!`, `~` |
| highest | `&` |
| | `^` |
| | `\|` |
| | `%` |
| | `*`, `/` |
| | `+`, `-` |
| | `<<`, `>>` |
| | `<`, `<=`, `>`, `>=`, `==`, `!=`, `is` |
| | `&&` |
| | `\|\|` |
| lowest | `??`, then `..`, then `? :` |
