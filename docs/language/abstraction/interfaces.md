---
title: Interfaces
summary: Name the methods a type must provide and use types interchangeably.
icon: material/connection
status: stable
targets: [native, web, node]
related: [language/abstraction/generics.md, language/types/classes-structs.md]
---

# Interfaces

An interface names the methods a type must provide. A call uses the implementation of the value you actually pass.

!!! glance "At a glance"

    ```dream
    interface Animal {
        fun speak(): string;
    }

    class Cat: Animal {
        public fun speak(): string {
            return "meow";
        }
    }
    ```

    - An interface lists the methods a type must provide.
    - Code written against an interface accepts any type that implements it.

## Declaring an interface

List method signatures. A signature with no body ends in a semicolon, and an interface has methods only:

```dream
interface Animal {
    fun speak(): string;
    fun legs(): int;
}
```

## Implementing an interface

A class implements interfaces by listing them after a colon, defining every method with a matching signature:

```dream
class Cat: Animal {
    public fun speak(): string {
        return "meow";
    }

    public fun legs(): int {
        return 4;
    }
}

class Dog: Animal {
    public fun speak(): string {
        return "woof";
    }

    public fun legs(): int {
        return 2;
    }
}
```

A class can implement several interfaces at once (`class Robot : Animal, Serializable { ... }`). A missing method, or the wrong signature, is a compile error.

## Using an interface-typed value

A class value is accepted wherever its interface is expected — this **upcast** is implicit and free.
The static type becomes the interface, but the value remembers its concrete class:

```dream
fun describe(a: Animal): void {
    System.println(a.speak()); // runs Cat.speak or Dog.speak
    System.println(a.legs());
}

describe(Cat()); // meow / 4
describe(Dog()); // woof / 2
```

You can store an interface value explicitly, with or without a cast:

```dream
let c = Cat();
let a: Animal = c; // implicit upcast
let b = (Animal)c; // explicit upcast — same value
```

An interface is an abstract contract, so instantiating it (`Animal()`) is an error.

## Interface inheritance

An interface may extend one or more parents with `: Parent` and `+` for multiples (same combinator as [generic bounds](generics.md)):

```dream
interface Collection<T> {
    get length(): int;
    fun iterator(): Iterator<T>;
}

interface IndexedCollection<T>: Collection<T> {
    fun get(index: int): T;
}

interface Readable {
    fun read(): int;
}

interface Writable {
    fun write(v: int): void;
}

interface ReadWrite: Readable + Writable {
    fun flush(): void;
}
```

A type that implements the child is a subtype of every parent, so `List<T>` can declare `IndexedCollection<T>` and still be used as `Collection<T>`. Inherited methods become part of the child, a same-named child method overrides the parent, and a cycle among parents is an error.

Ambiguous defaults from two parents (same method name, both with a default body, no override on the child) are rejected.

Class `implements` lists stay comma-separated (`class Foo : A, B`); only interface parents use `+`.

## Extending an interface

An `extend` block may target an interface — including a generic one — to add **package methods** that are not part of the interface contract:

```dream
// in system.collections
extend Collection<T> {
    public fun to_list(): List<T> {
        let result = List<T>();
        for (let x in this) {
            result.push(x);
        }
        return result;
    }
}
```

After `import system.collections;`, those methods are available on the interface type and on every implementer (`list.to_list()`, `(Collection<int>)xs.to_list()`).
Implementers do not need to override them.

## Default methods

A method may carry a **default body** that implementers inherit unless they override it.
A default can call the interface's other methods on `this`, which still go to the concrete type:

```dream
interface Greeter {
    fun name(): string;

    fun greet(): string { // default body
        return "Hello, I am " + this.name();
    }
}

class Person: Greeter {
    public fun name(): string {
        return "Ada";
    }
// inherits the default greet()
}

class Robot: Greeter {
    public fun name(): string {
        return "R2";
    }

    public fun greet(): string {
        return "BEEP " + this.name();
    } // overrides
}
```

## Checking the concrete type with `is`

Test what an interface value (or `object`) actually holds at runtime:

```dream
let a: Animal = Cat();
if a is Cat {
    System.println("it's a cat");
}
```

### `is` with binding

`is` can narrow and bind in one step — `expr is Type name` scopes `name: Type` to the guarded branch, with no separate cast.
It works for any target type, unboxing value types held in an `object`:

```dream
let a: Animal = Cat();
if a is Cat cat {
    System.println(cat.speak()); // `cat` aliases the same object
}
```

The bound name exists only inside the taken branch.

!!! note
`is` with a binding works in `if` and `while`, and in a top-level `&&` chain. `if (x is T t && t.ok())` is valid because `t` is read only after the test succeeds.

`t` is not available across `||`, so `if (x is T t || t.ok())` is an error.

## Advanced

### Implementing an interface with `extend`

An `extend` block can carry an `implements` clause, making an **existing** type — even a primitive — satisfy an interface:

```dream
extend int: Comparable<int> {
    public fun compare(other: int): int {
        if this < other {
            return 0 - 1;
        }
        if this > other {
            return 1;
        }
        return 0;
    }
}
```

This is exactly how the prelude makes primitives `Comparable` so `List<int>().sort()` works.
The type then satisfies [generic constraints](generics.md#generic-constraints) like `T : Comparable<T>`.

### Generic interfaces

An interface can be generic.
A class implements a concrete or generic instance of it; when a generic class implements a generic interface, its type parameter flows in:

```dream
interface Container<T> {
    fun get(): T;
    get length(): int;
}

class Box<T>: Container<T> {
    public value: T;

    public fun get(): T {
        return this.value;
    }

    public get length(): int {
        return 1;
    }
}
```

Each concrete use is specialized: `Box<int>` implements `Container<int>`.
Calls use that concrete interface type:

```dream
fun describe(c: Container<int>): void {
    System.println(c.get());
    System.println(c.length);
}

let b = Box<int>(7);
describe(b); // implicit upcast Box<int> -> Container<int>
let d = (Container<int>)b; // explicit upcast to a generic interface
```

### Async interface methods

An interface method may be `async`.
Calling it through an interface value you call the method on goes to the concrete async implementation, which returns a `Future<T>` to `await`:

```dream
interface Fetcher {
    async fun fetch(): int;
}

class Remote: Fetcher {
    public base: int;

    public async fun fetch(): int {
        Time.sleep(10).await;
        return this.base + 1;
    }
}

async fun run(f: Fetcher): void {
    let v = f.fetch().await; // await the Future<int>
    System.println(v);
}
```

An `async` interface method must be implemented by an `async` method (and non-async by non-async) — a mismatch is a compile error.

### Built-in `Equatable` and `Comparable`

Two generic interfaces ship in bootstrap `system.core` (no import):

```dream
interface Equatable<T> {
    fun equals(other: T): bool;
}

interface Comparable<T> {
    fun compare(other: T): int;
}
```

A type implements them against itself (`class Money : Comparable<Money>, Equatable<Money>`).
By convention `compare` returns a negative number, zero, or a positive number when `this` is ordered before, equal to, or after `other`.

Every numeric primitive plus `char` and `string` already implements `Comparable` (via prelude `extend` blocks), so with `import system.collections;` — `List<int>().sort()`, `binary_search`, and comparisons in generic code work with no extra code.

- **`==` / `!=` route to `equals`** when both operands are the same user type implementing `Equatable<Self>`. Primitives and strings keep built-in equality.
- **`<` / `<=` / `>` / `>=` use `compare`** when the left operand's type implements `Comparable<Self>` (`a < b` means `a.compare(b) < 0`). A more specific [`@operator("...")`](operator-overloading.md#operator-overloading) for a different symbol is unaffected.
- **`compare` powers sorting** via `List<T : Comparable<T>>.sort()` and `List<T>.sort_by(cmp)`. See [List sorting](../../library/collections/list.md#copy-join-and-sort).

Both interfaces work with [value structs](../types/classes-structs.md). A known concrete type calls that method directly, and assigning a value struct to an interface variable copies it onto the heap:

```dream
let a: Shape = Rect(3, 4); // boxed; a.area() runs the Rect implementation
```

## See also

- [Classes & Structs](../types/classes-structs.md) — defining types, methods, and visibility.
- [The `object` Type](../types/objects.md) — the universal container and the `is` operator.
