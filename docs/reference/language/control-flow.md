# Control Flow

Control flow decides which code runs and how often. Dream has `if`, loops, and `switch`, plus labeled loops for nested cases.

## if / else

Conditions must be `bool`. Parentheses around the condition are optional. Bodies may be a braced block or a single statement:

```dream
if score >= 90 {
    System.print("A\n");
} else if score >= 70 {
    System.print("B\n");
} else {
    System.print("F\n");
}

if ok return;
if flag System.print("yes\n"); else System.print("no\n");
```

For selecting a *value*, the ternary `cond ? a : b` is often cleaner — see [Operators](operators.md).

## Loops

### while

Runs the body while the condition holds. Parentheses around the condition are optional:

```dream
let i = 0;
while i < 10 {
    System.println(i);
    i++;
}
while done break;
```

### do / while

Same as `while`, but the condition is checked at the end, so the body always runs at least once:

```dream
let i = 0;
do {
    System.println(i);
    i = i + 1;
} while i < 3;
```

### for

A three-part loop: initializer, condition, increment. All three parts are optional. The initializer runs once, the condition is checked before each pass, and the increment runs after each body:

```dream
for (let i = 0; i < 5; i++) {
    System.println(i);
}
```

### for-each

Iterate a collection's elements directly with `for (let x in ...)`. The loop variable takes each element in turn:

```dream
let xs: int[] = [10, 20, 30];
for (let value in xs) {
    System.println(value);
}
```

`for..in` also works over a `string` (yielding each `char`), over any type implementing the enumerator protocol — including `List` and `Map` — and over interface-typed `Collection<T>`, `IndexedCollection<T>`, or `Iterator<T>` (dispatched through `.iterator()` / `.next()`). See [Indexers and enumerators](classes-structs.md#indexers-and-enumerators).

```dream
for (let c in "abc") {
    System.println(c);   // 'a', 'b', 'c'
}

fun sum(xs: Collection<int>): int {
    let total = 0;
    for (let n in xs) {
        total = total + n;
    }
    return total;
}
```

## break and continue

`break` leaves the nearest loop; `continue` skips to its next iteration:

```dream
for (let i = 0; i < 10; i = i + 1) {
    if i % 2 == 0 {
        continue;   // skip even numbers
    }
    System.println(i);
}
// break; continue;   // error: only allowed inside a loop
```

## checked and unchecked blocks

Integer arithmetic wraps by default. `checked { ... }` runs its statements with overflow panics; `unchecked { ... }` restores wrapping inside it. Both are ordinary blocks otherwise — they open a scope, and `return`/`break`/`continue` pass through them:

```dream
fun buffer_bytes(count: int, size: int): int {
    checked {
        return count * size;   // panics instead of returning a wrapped size
    }
}
```

See [Primitives § Integer overflow](primitives.md#integer-overflow) for exactly which operations check.

## switch

`switch` has two forms, and the parser picks based on the body:

- A **label switch** (below) starts with `case`/`default` and matches against constant labels.
- A **pattern-matching** switch uses `pattern => body` arms to destructure [discriminated unions](enums-unions.md).

### Label switch

A label switch has **no fallthrough** — each `case` runs only its own block. A case may list comma-separated labels, and `default` is optional:

```dream
switch (code) {
    case 1, 2:
        System.print("low\n");
    case 3:
        System.print("three\n");
    default:
        System.print("other\n");
}
```

Labels must be constants (integers, strings, booleans, or enum members) that match the subject's type. Duplicate labels are an error. Enums work naturally:

```dream
enum Color { Red, Green, Blue }

switch (c) {
    case Color.Red:   System.print("red\n");
    case Color.Green: System.print("green\n");
    default:          System.print("other\n");
}
```

### Pattern switch

For payload enums and unions, use pattern arms. Full rules live on [Enums & unions](enums-unions.md):

```dream
switch (opt) {
    Option.Some(v) => System.println(v),
    Option.None => System.println("empty"),
}
```

## Advanced: labeled loops

Give a loop a label so `break`/`continue` can target an outer loop from inside a nested one:

```dream
outer: for (let i = 0; i < 3; i = i + 1) {
    for (let j = 0; j < 3; j = j + 1) {
        if j == 1 {
            continue outer;   // next iteration of the outer loop
        }
        if i == 2 {
            break outer;      // exit both loops
        }
        System.println(i * 10 + j);
    }
}
```

The label must enclose this loop — targeting a label that is not an enclosing loop is a compile error.
