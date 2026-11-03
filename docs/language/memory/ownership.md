---
title: Ownership
summary: What happens when you assign a value or pass it to a function.
icon: material/key-outline
status: stable
targets: [native, web, node]
related: [language/memory/index.md, language/memory/safety.md, language/functions/parameters.md]
---

# Ownership

Ownership explains what happens when you assign a value or pass it to a function. Objects can be shared, while simple values and structs are copied. Dream handles cleanup automatically; use `borrow` when a call should use an object without taking ownership.

!!! glance "At a glance"

    ```dream
    let p = Point(1);
    let q = p; // same Point
    q.x = 9; // p.x is now 9 too
    ```

    - Objects are shared on assignment; simple values and structs are copied.
    - A function takes a parameter it keeps and borrows one it only reads; a last use moves.

This page is only about **who owns that value** when you pass or assign something. There is no `move` keyword. Dream moves for you when it can see you are done with a name.

## Two kinds of values

| Kind | Examples | `let b = a` |
|------|----------|-------------|
| **Share** (heap refs) | `class`, `string`, arrays, `List` / `Map` / `Set`, `js` | `a` and `b` are the **same** object |
| **Copy** (values) | `int`, `bool`, `struct` | `b` is an **independent** copy |

```dream
class Point {
    public x: int;

    public constructor(x: int) {
        this.x = x;
    }
}

let p = Point(1);
let q = p; // same Point
q.x = 9;
System.println(p.x); // 9
```

```dream
struct Vec2 {
    public x: int;
    public y: int;
}

let a = Vec2(1, 2);
let b = a; // copy
b.x = 9;
System.println(a.x); // 1
```

Automatic **move** (the rest of this page) applies to **share** types. A `struct` always copies its bytes. On the last use of a struct local, the copy keeps the nested heap values; they are not counted a second time. A struct that is still used later, or a field or index, still counts them again.

## Passing to a function

Write nothing on a parameter and Dream reads the function body to decide who owns the value:

- If the body **keeps** the parameter (stores it in a field, element or global, returns it, or
  passes it to another function that keeps it), the function **takes** it. This is a **sink**.
  When you still need the name after the call, Dream passes a copy instead of moving.
- Otherwise the function only **borrows** it: you keep the value and nothing is copied.

Recursive functions are handled too: a function that only passes a parameter to itself and reads
it borrows it.

Some parameters always take the value, whatever the body does:

- A parameter whose type has a `del` (or holds a value that does), so its destructor always runs
  when the function returns, never later. Function values and interfaces count as having one,
  because the compiler cannot see what they hold.
- Parameters of exported and `async` functions, of interface implementations, and of functions
  you pass around as values. Their callers cannot be checked, so they keep the written mode.

| On the parameter | Meaning |
|------------------|---------|
| *(nothing)* | Decided from the body: takes the value if the body keeps it, otherwise borrows it. |
| `sink` | Always takes the value, even when the body only reads it. |
| `borrow` | Always borrows: the callee reads it and you still own it. |
| `ref` | Alias the **variable** itself so the callee can assign to your local. See [Functions](../functions/parameters.md#ref-parameters). |

```dream
class Inbox {
    public last: string;

    public constructor() {
        this.last = "";
    }

    public fun keep(s: string): void {
        this.last = s; // stores s, so `keep` takes it
    }
}

fun show(s: string): void {
    System.println(s); // only reads s, so `show` borrows it
}

fun demo() {
    let inbox = Inbox();

    let a = "hi";
    show(a); // a stays yours
    inbox.keep(a); // last use of a → move, no extra copy

    let b = "yo";
    inbox.keep(b); // b is used again below → copy into keep
    show(b); // still valid
}
```

`List.push` (and similar stores) take their argument too:

```dream
let xs = List<string>();
let s = "item";
xs.push(s); // last use of s → move into the list
```

A method’s implicit `this` is **never** a sink. Calling `obj.foo()` does not consume `obj`.

### Results marked `borrow`

A few standard library methods, such as `Map.get` and `JsonValue.as_map`, show `borrow` before
their return type (`fun get(borrow key: K): borrow Option<V>`). The result is read straight out
of the collection instead of being copied for the call. You use and keep it like any other
value; Dream adds the copy only when the value outlives the line that reads it.

## Last-use move (no keyword)

Dream looks **forward**: after this line, is this name still read?

- **No** → **move**: hand the value to the destination. The old name is cleared so it cannot free the object twice.
- **Yes** → **copy**: both names stay valid.

That is the same rule for:

- a sink **argument**: `take(a)`
- a local **assign**: `let b = a;` when `a` is never used after that line
- a **return**: `return a;` hands the value to the caller (the function does not free `a`)

You do not write `move`. You also cannot force a move; if the name is still live, you get a copy.

Moves only come from a **local or sink-parameter name**. A field or `list[i]` used as a sink argument is always copied (the container still holds the value).

## The one error: use after a field store

Using a name after `take(a)` is fine (that was a copy). What **is** an error: a **sink parameter** stored into a **field or index**, then used by the old name.

The value now lives in the field. Read it there:

```dream
class Bag {
    items: List<int>;
    end: int;

    public constructor(items: List<int>) {
        this.items = items;
        this.end = this.items.length; // OK
    // this.end = items.length;     // error: use of 'items' after it was moved
    }
}
```

The error names the place that owns the value now and what to write instead:

```text
error: use of 'items' after it was moved
note: 'items' was moved into 'this.items' on line 6; 'this.items' owns it now
help: use 'this.items' here instead of 'items'
help: or read what you need from 'items' before the line that stores it
```

Giving a new value to the same name (`items = List<int>();`) makes the name usable again.

## Receiver modes: inferred borrow and unique

Every non-static method's implicit `this` has a **mutation contract** Dream infers from the body:

- **Borrow**: reads `this` without mutating it. Always safe to call.
- **Unique**: may mutate instance state (field writes, calls to other Unique methods).

You never write these — they are inferred from what the method does:

```dream
class Counter {
    count: int;

    // Inferred borrow: reads only.
    public fun value(): int {
        return this.count;
    }

    // Inferred unique: writes this.count.
    public fun increment(): void {
        this.count = this.count + 1;
    }
}
```

Two rules are enforced:

1. A method declared `borrow fun` cannot mutate `this` (the declaration is a contract).
2. When a class implements an interface, implementor modes must match.

Pin a contract explicitly with `borrow fun` / `unique fun` when inference isn't what you want:

```dream
class Counter {
    count: int;

    borrow fun value(): int {
        return this.count;
    }

    unique fun increment(): void {
        this.count += 1;
    }
}
```

## What is *not* a move

- Last **read**, then the function does other work: the value is still released at the **end** of the function, not the moment you finish reading it.
- `struct` assign or pass: always a byte copy. On the last use of a struct local, the copy keeps the nested heap values; they are not counted a second time. A field or `list[i]` still counts them again.
- Reading `obj.field` or `xs[i]` into a sink: always a copy; the object/array still owns its slot.
- `borrow` / `ref` arguments: never consumed.

## What to write in practice

- Parameters that **keep** the value (`print`, `length`, search): `borrow`.
- Parameters that **store** or **consume** the value (`push`, constructors, builders): leave unmarked.
- After `this.field = param`, use `this.field`, not `param`.

That is enough. Dream copies when needed and moves on last use.

See also [Memory](index.md) (when `del` runs, cycles, `weak`) and [Classes & structs](../types/classes-structs.md) (share vs copy).
