# Ownership

You never call `free`. Dream counts how many names still point at a heap value and frees it when the last one is gone ([Memory](memory.md)).

This page is only about **who owns that count** when you pass or assign something. There is no `move` keyword. The compiler moves for you when it can see you are done with a name.

## Two kinds of values

| Kind | Examples | `let b = a` |
|------|----------|-------------|
| **Share** (heap refs) | `class`, `string`, arrays, `List` / `Map` / `Set`, `js` | `a` and `b` are the **same** object |
| **Copy** (values) | `int`, `bool`, `struct` | `b` is an **independent** copy |

```dream
class Point {
    public x: int;
    public constructor(x: int) { this.x = x; }
}

let p = Point(1);
let q = p;      // same Point
q.x = 9;
println(p.x);   // 9
```

```dream
struct Vec2 {
    public x: int;
    public y: int;
}

let a = Vec2(1, 2);
let b = a;      // copy
b.x = 9;
println(a.x);   // 1
```

Automatic **move** (the rest of this page) applies to **share** types. A `struct` always copies its bytes; on last use of a struct **local**, nested share fields are not retained again (the destination inherits those counts). A still-live local, or a field/index, still retains nested share fields.

## Passing to a function

Write nothing extra on a parameter and the callee **takes** the value (a **sink**). If you still need the name afterward, the compiler **copies** (bumps the count) instead of moving.

Mark a parameter `borrow` when the function only **looks** — you keep the value.

| On the parameter | Meaning |
|------------------|---------|
| *(nothing)* | Sink — callee takes it. Move if this is the last use, otherwise copy. |
| `borrow` | Share — callee reads it; you still own it. |
| `ref` | Alias the **variable** itself so the callee can assign to your local. See [Functions](functions.md#ref-parameters). |

```dream
fun take(s: string): void {
    println(s);
}

fun peek(borrow s: string): void {
    println(s);
}

fun demo() {
    let a = "hi";
    take(a);        // last use of a → move, no extra copy

    let b = "yo";
    peek(b);        // borrow: b stays yours
    take(b);        // last use → move
}
```

If you use the name **after** a sink call, that call becomes a copy:

```dream
fun demo() {
    let b = "yo";
    peek(b);
    take(b);        // b is used again below → copy into take
    peek(b);        // still valid
}
```

`List.push` (and similar stores) are sinks too:

```dream
let xs = List<string>();
let s = "item";
xs.push(s);         // last use of s → move into the list
```

A method’s implicit `this` is **never** a sink. Calling `obj.foo()` does not consume `obj`.

## Last-use move (no keyword)

The compiler looks **forward**: after this line, is this name still read?

- **No** → **move**: hand the existing count to the destination. The old name is cleared so it cannot free the object twice.
- **Yes** → **copy**: bump the count; both names stay valid.

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
        this.end = this.items.length;   // OK
        // this.end = items.length;     // error: 'items' was moved into the field
    }
}
```

Giving a new value to the same name (`items = List<int>();`) makes the name usable again.

## What is *not* a move

- Last **read**, then the function does other work: the value is still released at the **end** of the function, not the moment you finish reading it.
- `struct` assign or pass: always a byte copy. Last use of a struct **local** does not retain nested share fields; a field or `list[i]` still does.
- Reading `obj.field` or `xs[i]` into a sink: always a copy; the object/array still owns its slot.
- `borrow` / `ref` arguments: never consumed.

## What to write in practice

- Parameters that **keep** the value (`print`, `length`, search): `borrow`.
- Parameters that **store** or **consume** the value (`push`, constructors, builders): leave unmarked.
- After `this.field = param`, use `this.field`, not `param`.

That is enough. The compiler inserts the retains and the last-use moves.

See also [Memory](memory.md) (when `del` runs, cycles, `weak`) and [Classes & structs](classes-structs.md) (share vs copy).
