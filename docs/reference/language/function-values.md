# Function values and closures

Store a function, pass one to another function, or create a function that remembers values from its surroundings.

[Back to overview](functions.md)

## Advanced

### Generic functions

Add `<TypeParam>` after the name. A generic function works for each concrete type you use, with no
extra cost when the program runs. See [Generics](generics.md).

```dream
fun identity<T>(value: T): T {
    return value;
}

System.println(identity<int>(42));
System.println(identity<string>("hello"));

fun pair_first<A, B>(a: A, b: B): A { return a; }
```

### First-class functions

A function name is a value; its type is written `fun(ParamTypes): ReturnType`. Store functions in variables, pass them, and call them like any other:

```dream
fun twice(x: int): int { return x * 2; }

fun apply(f: fun(int): int, value: int): int {
    return f(value);
}

let g: fun(int): int = twice;
System.println(g(5));            // 10
System.println(apply(twice, 8)); // 16
```

A [generic function used as a first-class value](generics.md#generic-functions-as-first-class-values) needs a `fun(...)`-typed context so its type arguments can be inferred — e.g. `let cmp: fun(int, int): int = natural_order;` — a bare `let f = natural_order;` is an error.

#### Arrow-lambda literals

An anonymous function can be written inline with arrow syntax, `(params) => expr` or `(params) => { statements }`. A parameter's `: Type` annotation is optional when the lambda is used in a `fun(...)`-typed context (a `let` annotation, or a parameter/argument whose declared type is `fun(...)`): omitted parameter types and the return type are taken from that context. When every parameter is explicitly annotated, the return type can instead be inferred from the body — so `let f = (x: int) => x * 2;` works without a surrounding `fun(...)` annotation. A lambda with any untyped parameter and no `fun(...)` context is rejected.

```dream
let add: fun(int, int): int = (x, y) => x + y;   // x, y inferred as int from the `let` annotation
System.println(add(2, 3));   // 5

let square: fun(int): int = (x) => {
    let r = x * x;
    return r;
};
System.println(square(5));   // 25

let nums: List<int> = List<int>();
nums.push(3);
nums.push(1);
nums.push(2);
nums.sort_by((a, b) => a - b);   // `a`/`b` inferred as `int` from `sort_by`'s `cmp: fun(int, int): int`
```

A lambda written with an untyped parameter and no surrounding `fun(...)` context cannot have its type inferred and is rejected with an error asking for one. A lambda may declare its own type parameters (`<T>(x: T) => x`), which work for each concrete type you use — from a `fun(...)` context or by binding a generic item and using it at each site.

#### Async lambdas

Prefix an arrow lambda with `async` to allow `await` in its body. An async lambda is a first-class value of type `fun(...): Future<T>` — calling it returns a lazy `Future` handle, same as calling a named `async fun` (the body runs on await / `Promise.start` / a combinator):

```dream
async fun main(): void {
    let twice: fun(int): Future<int> = async (x) => {
        Time.sleep(1).await;
        return x * 2;
    };
    let n = twice(21).await;   // twice(21) : Future<int>
    System.println(n);                // 42
}
```

The expected context must be `fun(...): Future<T>` (not `fun(...): T`). A sync lambda against a `Future`-returning `fun` type, or an async lambda against a non-`Future` return, is a compile-time error. Named `async fun` values work the same way — `let f: fun(int): Future<int> = delayed_triple;` boxes the function as a `Future`-returning `fun` value.

#### Capturing closures

A lambda's body may also reference variables from an enclosing function; this is a *capture*. A captured name is captured **by reference**, not by value: the closure and the enclosing function share the same storage, so a write from either side is visible to the other, and the closure keeps working after the enclosing function has returned. A lambda may capture more than one name, and capture is transitive: a lambda nested inside another lambda may reach past its immediate parent to a grandparent's (or higher) local — each level forwards what the level below it needs, one hop at a time.

```dream
fun make_adder(n: int): fun(int): int {
    return (x) => x + n;   // `x` inferred from the return type; captures the parameter `n`
}

let add5: fun(int): int = make_adder(5);
System.println(add5(10));   // 15
System.println(add5(20));   // 25

fun make_counter(): fun(): int {
    let count: int = 0;
    return () => {
        count = count + 1;   // mutates the enclosing `let` — visible next call, and to `count`
        return count;        // itself if it's still in scope when this returns
    };
}

let inc: fun(): int = make_counter();
System.println(inc());   // 1
System.println(inc());   // 2
```

Each call to a function that returns a capturing lambda creates its own independent storage — two counters from separate `make_counter()` calls do not interfere with each other.

Capturing closures are ordinary `fun(...)` values. The closure keeps the values it captured until nothing uses the closure anymore. A self-capturing closure (a `fun` that stores itself into its own environment) can still form a reference cycle and leak, just like mutually-referencing classes — break such cycles deliberately or avoid them.

Capturing closures **cannot** be passed to JavaScript APIs — the JS bridges drop the closure environment. See [Callbacks](callbacks.md).

See [`ref` and closures](function-parameters.md#ref-and-closures) for how a captured variable composes with a `ref` parameter on another function (they share the same underlying storage), and why a lambda cannot capture an enclosing `ref` parameter itself.

Capturing more than one variable, and reaching past an immediate parent lambda to a grandparent's local, both work the same way:

```dream
let a: int = 1;
let b: int = 2;
let f: fun(): int = () => a + b;   // captures both `a` and `b`
System.println(f());   // 3

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
System.println(l2());   // 104
System.println(l2());   // 105
```

### Overloading

Multiple functions can share a name if their parameters differ; see [Language Invariants](invariants.md#overloading). An exact-arity match wins over one that fills in a default, and truly ambiguous calls are reported.
