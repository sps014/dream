# Language tour

Four short programs. Each comment (and the numbered notes) explains one idea. Run them with `dreamer run` inside a project, or `dream run file.dream`.

## 1. Variables

A **variable** is a named box that holds a value. `let` can be changed later. `const` cannot.

```dream
import system;                 // (1)

fun main() {                   // (2)
    let name = "Ada";          // (3)
    const n = 3;               // (4)
    name = "Grace";            // (5)
    System.println(name);      // (6)
    System.println(n);
}
```

1. Bring in console I/O (`System.println`).
2. Every program starts in `main`.
3. `let` — the compiler sees `"Ada"` and treats `name` as text (`string`).
4. `const` — `n` is locked after this line.
5. Reassign a `let` with `=`.
6. Print a line to the terminal.

You can write the type yourself when you want: `let score: int = 10;`.

See [Variables](../reference/language/variables.md).

## 2. Control flow

**Control flow** chooses which lines run, and how often.

```dream
import system;

fun main() {
    let score = 85;

    if (score >= 90) {         // (1)
        System.println("A");
    } else if (score >= 70) {
        System.println("B");
    } else {
        System.println("C");
    }

    let i = 0;
    while (i < 3) {            // (2)
        System.println(i);
        i = i + 1;             // (3)
    }
}
```

1. `if` needs parentheses. The condition must be `true` or `false`.
2. `while` repeats the block as long as the condition is true.
3. Change `i` or the loop never ends.

`for (let i = 0; i < 3; i = i + 1)` is another way to count. `for (let x in list)` walks a collection.

See [Control flow](../reference/language/control-flow.md).

## 3. Functions

A **function** is a named recipe. You pass values in; it can `return` a value out.

```dream
import system;

fun greet(name: string): string {   // (1)
    return "Hello, " + name;        // (2)
}

fun main() {
    let msg = greet("world");       // (3)
    System.println(msg);
}
```

1. `fun`, then the name, then `name: type` parameters, then `: ReturnType`.
2. `+` on strings glues them together.
3. Call `greet` with `"world"`; `msg` is `"Hello, world"`.

`fun main()` has no return type — it returns nothing.

See [Functions](../reference/language/functions.md).

## 4. Lists

A **list** grows as you add items. Import `system.collections` for `List`.

```dream
import system;
import system.collections;     // (1)

fun main() {
    let xs = List<int>();      // (2)
    xs.push(10);               // (3)
    xs.push(20);

    for (let n in xs) {        // (4)
        System.println(n);
    }
}
```

1. `List`, `Map`, and `Set` live in this package.
2. `List<int>()` — an empty list that only holds integers.
3. `push` adds at the end.
4. `for..in` sets `n` to each element.

See [Arrays](../reference/language/arrays.md) and [Collections](../reference/stdlib/collections.md).

## What next?

[Next steps](next-steps.md) — reference, cookbook, and community.
