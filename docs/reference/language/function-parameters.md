# Pass values to functions

Choose defaults, named arguments, repeated arguments, and the right way to share or change a value.

[Back to overview](functions.md)

## Default parameter values

A parameter can supply a default with `= <literal>`; callers may then omit it:

```dream
fun greet(name: string, times: int = 1): void {
    let i = 0;
    while i < times {
        System.println("hi " + name);
        i = i + 1;
    }
}

greet("Ada");      // times = 1
greet("Ada", 3);   // times = 3
```

Rules:

- A default must be a **constant**: a number (may be negative), `true`/`false`, a string, a char, or a unit enum/union construction such as `Option.None`. No arbitrary expressions.
- Defaults must be **trailing** — once one parameter has a default, all after it must too.
- Callers must still pass every leading required argument; passing more than the total is an error.

Defaults also apply to constructors and methods:

```dream
class Greeter {
    public factor: int;
    public constructor(factor: int = 3) { this.factor = factor; }
    public fun scale(n: int, by: int = 2): int { return n * by * this.factor; }
}

let g = Greeter();        // factor = 3
System.println(g.scale(4));      // 4 * 2 * 3 = 24
System.println(g.scale(4, 5));   // 4 * 5 * 3 = 60
```


## Named arguments

A call may label an argument with its parameter's name (`name: value`) instead of relying on
position:

```dream
fun greet(name: string, greeting: string = "Hello", punctuation: string = "!"): string {
    return greeting + ", " + name + punctuation;
}

greet(name: "Ada");                          // "Hello, Ada!"
greet(name: "Lin", greeting: "Hi");          // "Hi, Lin!"
greet(greeting: "Hi", name: "Grace");        // "Hi, Grace!" — order doesn't matter
greet("Bob", punctuation: "?");              // "Hello, Bob?" — positional + named mixed
greet(name: "Amy", punctuation: ".");        // "Hello, Amy." — skips `greeting`, its default fills in
```

Rules:

- **Every positional argument must come before every named one** — `greet(name: "Ada", "Hi")` is
  an error.
- A name must match a declared parameter of the callee; each name may be used **at most once** per
  call.
- Naming an argument doesn't require naming every argument after it — any parameter left unfilled
  once positional and named arguments are assigned falls back to its default value (an error if it
  has none).
- Named arguments work for **free functions, constructors, and instance methods**, including
  overloaded callees: each overload's parameter names are tried, then argument types pick among
  the survivors (an ambiguous name layout across overloads is an error). Use positional arguments
  when you prefer not to involve names.

Named arguments and defaults compose: they're the mechanism that lets a call skip a *middle*
optional parameter while still supplying a later one, which trailing-only default omission alone
cannot express:

```dream
class Rect {
    public width: int;
    public height: int;
    public constructor(width: int, height: int = 1) { this.width = width; this.height = height; }
    public fun area(scale: int = 1, offset: int = 0): int {
        return (this.width * this.height * scale) + offset;
    }
}

let r = Rect(width: 3, height: 4);
System.println(r.area(offset: 5));   // scale keeps its default (1): 3*4*1 + 5 = 17
```


## Variadic parameters

A function's **last** parameter can be marked `...name: T[]` to accept zero or more trailing `T`
arguments, collected into an array bound to `name` inside the body:

```dream
fun sum(...nums: int[]): int {
    let total = 0;
    for (let n in nums) {
        total = total + n;
    }
    return total;
}

sum();          // total = 0  (nums is an empty array)
sum(1);         // total = 1
sum(1, 2, 3);   // total = 6
```

A variadic parameter can follow ordinary (including defaulted) parameters, as long as it is last:

```dream
fun sum_with_base(base: int, ...nums: int[]): int {
    let total = base;
    for (let n in nums) {
        total = total + n;
    }
    return total;
}

sum_with_base(10);           // 10
sum_with_base(10, 1, 2, 3);  // 16
```

Rules:

- Only the **last** parameter may be variadic; a required or defaulted parameter cannot follow it.
- Its declared type must be an array type (`T[]`); the caller passes bare `T` values, not an
  already-built array — there is exactly one calling convention, not two.
- Variadic parameters work for **free functions, constructors, and instance methods**, including
  overloaded callees: trailing arguments are matched against the variadic element type during
  overload resolution, then packed into the `T[]` parameter.
- Named arguments and variadic parameters compose in one call: name the **fixed** parameters, then
  supply any trailing variadic elements positionally — e.g. `sum_with_base(base: 10, 1, 2, 3)`.
  The variadic parameter itself cannot be passed by name (there is one calling convention: bare
  `T` values, not a pre-built array).


## Ownership: sink-default and `borrow`

Unmarked parameters **sink**; `borrow` shares; last-use **moves** with no keyword. Full guide:
[Ownership](ownership.md).

| Modifier | Meaning |
|----------|---------|
| *(none)* | Sink — callee takes it (move if last use, else copy) |
| `borrow` | Callee reads it; you keep it |
| `ref` | Mutable place alias ([below](#ref-parameters)) |

The implicit `this` value you call the method on is never a sink.


## `ref` parameters {#ref-parameters}

```dream
fun swap(ref a: int, ref b: int): void {
    let tmp: int = a;
    a = b;
    b = tmp;
}

let p: int = 1;
let q: int = 2;
swap(ref p, ref q);
System.println(p);   // 2
System.println(q);   // 1
```

`ref` works the same way on instance and static methods (the implicit `this` is unaffected — `ref`
only ever applies to the explicit parameter list):

```dream
struct Doubler {
    public fun apply(ref x: int): void {
        x = x * 2;
    }
}

let d: Doubler = Doubler();
let n: int = 5;
d.apply(ref n);
System.println(n);   // 10
```

Rules:

- `ref` must appear at the call site (`f(ref x)`), not just the declaration — omitting it, or
  adding it where the parameter isn't `ref`, is a compile-time error.
- A `ref` argument's target may be a local variable, a parameter, a struct/class field, or an
  array element (`f(ref x)`, `f(ref obj.field)`, `f(ref arr[i])`).
- `ref` cannot combine with a default value or a variadic (`...`) parameter on the same parameter.
- A lambda may declare `ref` parameters. Annotate the function type as `fun(ref T): R` (the
  `ref` markers are part of the type) and call with `f(ref x)`:
  ```dream
  let inc: fun(ref int): void = (ref n: int) => { n = n + 1; };
  let a: int = 5;
  inc(ref a);
  System.println(a); // 6
  ```
- A lambda **cannot capture** an enclosing function's `ref` parameter, even though it can capture
  an ordinary `let`/parameter (see [Capturing closures](function-values.md#capturing-closures)). A `ref`
  parameter's storage is only guaranteed to live for the duration of the call it came from; a
  capturing lambda could outlive that call (e.g. by being returned), which would leave it holding
  a dangling reference. This is a compile-time error.

### `ref` and closures {#ref-and-closures}

A `ref` argument aliases the caller's storage. If that local is also captured by a closure, both
see the same storage — mutations through the `ref` and through the closure are visible to both:

```dream
fun increment(ref x: int): void {
    x = x + 1;
}

fun main(): void {
    let counter: int = 0;
    let inc: fun(): int = () => {
        increment(ref counter);   // ref-passes the closure's own captured storage
        return counter;
    };
    System.println(inc());     // 1
    System.println(inc());     // 2
    System.println(counter);   // 2 — visible to the enclosing scope too

    increment(ref counter);   // and the enclosing scope's writes are visible to the closure
    System.println(inc());           // 4
}
```
