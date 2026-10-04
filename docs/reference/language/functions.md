# Functions

A function gives a reusable task a name. Declare it with `fun`, describe the values it accepts, and give its return type when it produces a result.

## Explore this topic

- [Pass values to functions](function-parameters.md)
- [Function values and closures](function-values.md)

## Defining and calling

```dream
fun add(a: int, b: int): int {
    return a + b;
}

let result = add(3, 4);
```

- `fun`, then the name.
- Parameters are `name: type`, comma-separated.
- `: ReturnType` follows the parameter list.

The return type is optional for functions that return nothing, so these are equivalent:

```dream
fun greet() { System.println("hi"); }
fun greet(): void { System.println("hi"); }
```

## Returning a value

Use `return`. Dream checks that every path returns when the return type is not `void`:

```dream
fun clamp(value: int, lo: int, hi: int): int {
    if value < lo { return lo; }
    if value > hi { return hi; }
    return value;
}
```

In a `void` function a bare `return;` exits early:

```dream
fun log_positive(n: int): void {
    if n < 0 { return; }
    System.println(n);
}
```

Functions can call themselves — recursion works as expected:

```dream
fun fib(n: int): int {
    if n <= 1 { return n; }
    return fib(n - 1) + fib(n - 2);
}
```

## Public functions and entry point

Functions are **file-private by default**. Mark one `public` to import it from other files and export it to the WebAssembly host (see [Imports](imports.md#visibility)). A `public` function cannot expose a non-`public` class:

```dream
// public fun make(): Secret { ... }   // error if Secret is not public
public fun compute(n: int): int {
    return n * n;
}
```

The runtime starts a program by calling `main`. Every runnable program needs one; its return type can be omitted:

```dream
fun main() {
    System.println("hello");
}
```
