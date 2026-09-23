# Generics

Generics let you write code once and use it for many types.
Dream picks a concrete type at compile time for each use.
There is no boxing and no extra runtime cost.

## Generic functions

Add `<T>` after the function name.
Dream usually infers `T` from the call.
You can also write the type explicitly with `<Type>`:

```dream
fun first<T>(arr: T[]): T {
    return arr[0];
}

let nums = [10, 20, 30];
let words = ["a", "b", "c"];
System.println(first<int>(nums));      // 10
System.println(first(words));          // "a" (inferred)
```

Multiple type parameters are allowed: `fun swap<A, B>(a: A, b: B): A { ... }`.

## Generic classes

Classes and structs can be generic.
Type arguments can themselves be generic or arrays, so generics nest freely:

```dream
class Pair<A, B> {
    first: A;
    second: B;
}

let p = Pair<int, string>(1, "one");
System.println(p.first);   // 1

let nested = Pair<Box<int>, int>(Box<int>(7), 5);
System.println(nested.first.v);   // 7
```

## Generic methods

A method automatically sees its class's type parameters:

```dream
class Box<T> {
    value: T;
    fun get(): T { return this.value; }
    fun set(v: T): void { this.value = v; }
}

let b = Box<int>(42);
b.set(100);
System.println(b.get());   // 100
```

## Advanced

### Generic constraints

Constrain a type parameter to one or more interfaces with `T : Iface`.
Inside the body, that interface's methods are available.
Constraints apply to functions, classes, structs, interfaces, and `extend` blocks:

```dream
fun max_of<T : Comparable<T>>(a: T, b: T): T {
    if a.compare(b) > 0 { return a; }   // compare available because T : Comparable<T>
    return b;
}
```

Combine bounds with `+`:

```dream
struct Sorted<T : Comparable<T> + Equatable<T>> { /* ... */ }
```

Kind constraints include `struct`, `class`, `unmanaged`, and `shared`.
A `shared` type is a blittable value, `string`, a struct of shared fields, or a `shared class`:

```dream
fun send<T : shared>(value: T): void { /* … */ }
```

At each use, Dream checks that the concrete type satisfies the constraint.
Otherwise you get a compile error (for example `List<int>().sort()` needs `int : Comparable<int>`).
Each type gets its own copy of the generic, so a constrained call binds to the concrete method with **no boxing** — even for [value structs](classes-structs.md).

There is no extra runtime wrapper for value structs.

### Static methods on a generic class

If the class type parameters appear in the static method's parameters, they are inferred from the call — the same rule as generic functions (`first(words)`).
Write `List.from_array(items)` or `Task.spawn(() => n * n)` (`TOut` is a method type argument).
Explicit `Class<Args>.method(...)` always works, and is required when a parameter does not appear in the arguments (`Cache.make` below):

```dream
class Cache<T> {
    seed: int;
    public constructor(seed: int) { this.seed = seed; }
    public static fun make(seed: int): Cache<T> {
        return Cache<T>(seed);
    }
}

let c = Cache<int>.make(5);   // T is not in make's parameters — write it on the class
System.println(c.seed);              // 5
```

As with any static member, the method must be `public` to be called from another file, and the generic class itself must be `public` to be referenced across files. See [visibility](imports.md).

### Generic functions as first-class values

A generic function can become a [first-class function value](functions.md).
With a concrete `fun(...)` type at the use site, its type arguments are inferred and a separate copy is used for that type.
A bare binding stays generic and picks a type independently at each later use (typed assignment, typed argument, or call):

```dream
fun natural_order<T : Comparable<T>>(a: T, b: T): int {
    return a.compare(b);
}

let cmp: fun(int, int): int = natural_order;   // inferred as natural_order<int>
let f = natural_order;                         // polymorphic until used
System.println(f(3, 1));                              // instantiates from argument types
let g: fun(int, int): int = f;                 // instantiates from the annotation
```

### Type checking inside generic bodies

Use `is` to branch on the concrete type.
The compiler eliminates the dead branches:

```dream
fun describe<T>(v: T): void {
    if v is int {
        System.print("it's an int: ");
        System.println(v);
    } else if (v is string) {
        System.print("it's a string: ");
        System.println(v);
    }
}
```

### How it works

Every unique combination of type arguments creates a new copy.
`Box<int>` and `Box<string>` are entirely separate types — no boxing, no overhead versus writing the type-specific code by hand.
