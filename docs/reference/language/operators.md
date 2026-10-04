# Operators

Operators calculate values, compare them, or update a variable. Start with the everyday operators below. The later sections explain custom operators and the order in which expressions are evaluated.

## Explore this topic

- [Define operators for your types](operator-overloading.md)
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
let x = 7 / (float)2;   // 3.5
```

Prefix `-` negates a number: `let neg = -x;`.

Integer arithmetic (`+`, `-`, `*`, `/`, `%`, `<<`, `>>`, and unary `-`) **wraps** at the type's
bit width on overflow rather than widening; `/` and `%` panic on a zero divisor. Inside a
`checked { }` block the same operators panic on overflow instead — see
[Primitives § Integer overflow](primitives.md#integer-overflow) for the full policy.

## String concatenation

When either side of `+` is a `string`, the other side is converted through its [`to_string`](../stdlib/builtins.md). A simple enum renders its variant *name*, not the number:

```dream
let msg = "Hello, " + name + "!";
let line = "color = " + Color.Green;   // "color = Green"
```

## String interpolation

Prefix a string with `$` and wrap expressions in `{ ... }`. Each hole is evaluated and converted to a string, just like `+`:

```dream
let name = "Ada";
let count = 3;
let msg = $"{name} has {count + 1} items";   // "Ada has 4 items"
```

Interpolation expands to concatenation, so the above equals `"" + name + " has " + (count + 1) + " items"`.

Double a brace to write it literally — `{{` produces `{`, `}}` produces `}`:

```dream
let x = 5;
let s = $"{{literal}} and {x}";   // "{literal} and 5"
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
false && boom();   // boom() never runs
true || boom();    // boom() never runs
```

## Bitwise

`&` (and), `|` (or), `^` (xor), `<<` (shift left), `>>` (shift right), and prefix `~` (complement)
work on any integer type: `int`, `uint`, `long`, `ulong`, `byte`. Both operands of a binary bitwise
op must be the same type, same as arithmetic. Simple enums are integers at runtime, so `&`/`|`/`^`
and prefix `~` also work on them and yield the same enum type (`Flags.Read | Flags.Write`). Shifts
stay integer-only. `>>` is an *arithmetic* (sign-extending) shift on the
signed types (`int`, `long`) and a *logical* (zero-filling) shift on the unsigned types (`uint`,
`ulong`, `byte`).

```dream
let flags: uint = 6u;           // 0b0110
let masked = flags & 4u;        // 4u   (0b0100)
let shifted: byte = 200b >> 2b; // 50b, zero-filled
let inverted = ~5;              // -6 (two's complement)
let inverted_b: byte = ~5b;     // 250b (wraps within byte's 0..255 range)
```

Like arithmetic, `~` and the binary bitwise ops on `byte` wrap their result into `byte`'s `0..255`
range — see [Primitives § Integer overflow](primitives.md#integer-overflow).

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
enclosing function immediately. See [Option & Result](../stdlib/option-result.md#try-propagation)
for the full rules.

```dream
fun quarter(n: int): Result<int, string> {
    let h = half(n)?;
    return Result.Ok(half(h)?);
}
```

Postfix `?` wins over ternary unless a matching `:` follows at the same nesting depth.
Example: `half(n)? + 1` is try-propagation; `cond ? a : b` is still the ternary.

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
total += 5;   // total = total + 5
count++;
++i;
let prev = j++;
let next = ++j;
for (let k = 0; k < n; k++) { }
```

Discard a value without binding a name using `_` (like a pattern wildcard):

```dream
let _ = sideEffect();
let (_, y) = pair;
let _ = fetch().await;
```

Unread `let`/`const` locals produce a warning (compile still succeeds). Use `_` when the value is intentionally ignored.

Any expression can be used as a statement (`expr;`); the result is evaluated and dropped.

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
| lowest | `??`, then `? :` |
