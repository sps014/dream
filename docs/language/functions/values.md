---
title: Function values
summary: "Store and pass functions, and capture surrounding values in closures."
icon: material/lambda
status: stable
targets: [native, web, node]
---

# Function values and closures

Store a function, pass one to another function, or create a function that remembers values from its surroundings.

!!! glance "At a glance"

    ```dream
    let add: fun(int, int): int = (x, y) => x + y;
    nums.sort_by((a, b) => a - b);
    ```

    - Functions are values: store them, pass them, and return them.
    - A closure remembers the values it captured from its surroundings.

[Back to overview](declaring.md)

## Advanced

### Generic functions

Add `<TypeParam>` after the name so the function works for each concrete type you use. See [Generics](../abstraction/generics.md).

```dream
fun identity<T>(value: T): T {
    return value;
}

System.println(identity<int>(42));
System.println(identity<string>("hello"));

fun pair_first<A, B>(a: A, b: B): A {
    return a;
}
```

### First-class functions

A function name is a value; its type is written `fun(ParamTypes): ReturnType`. Store functions in variables, pass them, and call them like any other:

```dream
fun twice(x: int): int {
    return x * 2;
}

fun apply(f: fun(int): int, value: int): int {
    return f(value);
}

let g: fun(int): int = twice;
System.println(g(5)); // 10
System.println(apply(twice, 8)); // 16
```

A [generic function used as a first-class value](../abstraction/generics.md#generic-functions-as-first-class-values) needs a `fun(...)`-typed context so its type arguments can be inferred — e.g. `let cmp: fun(int, int): int = natural_order;` — a bare `let f = natural_order;` is an error.

#### Arrow-lambda literals

An anonymous function is `(params) => expr` or `(params) => { statements }`.

- Omit `: Type` when a `fun(...)` type is already expected, such as a `let` annotation or a `fun(...)` parameter. Parameter and return types come from that context.
- When every parameter has a type, the return type comes from the body, so `let f = (x: int) => x * 2;` needs no `fun(...)` annotation.
- A lambda with an untyped parameter and no `fun(...)` context is rejected.

```dream
let add: fun(int, int): int = (x, y) => x + y; // x, y inferred as int from the `let` annotation
System.println(add(2, 3)); // 5

let square: fun(int): int = (x) => {
    let r = x * x;
    return r;
};
System.println(square(5)); // 25

let nums: List<int> = List<int>();
nums.push(3);
nums.push(1);
nums.push(2);
nums.sort_by((a, b) => a - b); // `a`/`b` inferred as `int` from `sort_by`'s `cmp: fun(int, int): int`
```

A lambda may declare its own type parameters, as in `<T>(x: T) => x`. Use it where a `fun(...)` type is expected, or bind it and pick a type at each call.

#### Async lambdas

Prefix an arrow lambda with `async` to allow `await` in its body. An async lambda is a first-class value of type `fun(...): Future<T>` — calling it returns a lazy `Future` handle, same as calling a named `async fun` (the body runs on await / `Promise.start` / a combinator):

```dream
async fun main(): void {
    let twice: fun(int): Future<int> = async (x) => {
        Time.sleep(1).await;
        return x * 2;
    };
    let n = twice(21).await; // twice(21) : Future<int>
    System.println(n); // 42
}
```

The expected type must be `fun(...): Future<T>`, not `fun(...): T`. A sync lambda cannot fill a `Future` return, and an async lambda cannot fill a plain return.

A named `async fun` uses the same type: `let f: fun(int): Future<int> = delayed_triple;`.

#### Capturing closures

A lambda may use variables from an enclosing function. That capture is shared: a write from either side is visible to the other, and the closure still works after the enclosing function returns.

A lambda may capture several names. A nested lambda can use a local from further out.

```dream
fun make_adder(n: int): fun(int): int {
    return (x) => x + n; // `x` inferred from the return type; captures the parameter `n`
}

let add5: fun(int): int = make_adder(5);
System.println(add5(10)); // 15
System.println(add5(20)); // 25

fun make_counter(): fun(): int {
    let count: int = 0;
    return () => {
        count = count + 1; // mutates the enclosing `let` — visible next call, and to `count`
        return count; // itself if it's still in scope when this returns
    };
}

let inc: fun(): int = make_counter();
System.println(inc()); // 1
System.println(inc()); // 2
```

Each call to a function that returns a capturing lambda creates its own independent storage — two counters from separate `make_counter()` calls do not interfere with each other.

A closure is an ordinary `fun(...)` value and keeps what it captured until nothing uses it. A closure that stores itself can leak, so avoid that or break the cycle on purpose.

Capturing closures **cannot** be passed to JavaScript APIs — the JS bridges drop the closure environment. See [Callbacks](../interop/callbacks.md).

See [`ref` and closures](parameters.md#ref-and-closures) for how a captured variable composes with a `ref` parameter on another function (they share the same underlying storage), and why a lambda cannot capture an enclosing `ref` parameter itself.

Capturing more than one variable, and reaching past an immediate parent lambda to a grandparent's local, both work the same way:

```dream
let a: int = 1;
let b: int = 2;
let f: fun(): int = () => a + b; // captures both `a` and `b`
System.println(f()); // 3

fun make(a: int, b: int): fun(): fun(): int {
    let c: int = 100;
    // The outer lambda doesn't reference `a`/`b`/`c` itself, but forwards all three to the inner
    // one, which does — a multi-level, multi-capture chain.
    return () => {
        return () => {
            c = c + 1;
            return a + b + c;
        };
    };
}

let l1: fun(): fun(): int = make(1, 2);
let l2: fun(): int = l1();
System.println(l2()); // 104
System.println(l2()); // 105
```

### Overloading

Multiple functions can share a name if their parameters differ; see [Language Invariants](../rules/invariants.md#overloading). An exact-arity match wins over one that fills in a default, and truly ambiguous calls are reported.
