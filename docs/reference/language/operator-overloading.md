# Define operators for your types

Give a custom type useful arithmetic, comparison, or conversion behavior.

[Back to overview](operators.md)

## Operator overloading

A class or struct can give `+`, `-`, `*`, `/`, `%`, `&`, `|`, `^`, `<<`, `>>`, `==`, unary `-`, `!`,
`~`, and both implicit and explicit casts their own meaning with `fun operator +(...)` and
`fun implicit(): T` / `fun explicit(): T`.

```dream
class Vector2 {
    public x: int;
    public y: int;

    public constructor(x: int, y: int) {
        this.x = x;
        this.y = y;
    }

    fun operator +(other: Vector2): Vector2 {
        return Vector2(this.x + other.x, this.y + other.y);
    }

    fun operator -(other: Vector2): Vector2 {
        return Vector2(this.x - other.x, this.y - other.y);
    }

    // A one-parameter method takes the binary `-`; a zero-parameter method (same symbol) takes
    // unary `-`. Arity tells them apart.
    fun operator -(): Vector2 {
        return Vector2(-this.x, -this.y);
    }

    fun operator ==(other: Vector2): bool {
        return this.x == other.x && this.y == other.y;
    }
}

fun main(): void {
    let a = Vector2(1, 2);
    let b = Vector2(3, 4);
    let c = a + b;        // Vector2(4, 6)
    let d = -a;            // Vector2(-1, -2)
    let same = a == a;     // true
    let diff = a != b;     // true — `!=` reuses `operator ==`, negated
}
```

Rules:

- A tagged method's own parameter list fixes the operator's arity: one parameter is a binary
  overload (the right-hand operand), zero parameters is a unary overload. `+`/`*`/`/`/`%`/the
  bitwise operators/`==` are binary-only; `!`/`~` are unary-only; `-` may be either.
- `!=` has no operator of its own — a registered `operator ==` also powers it, negated.
- `<`, `<=`, `>`, `>=` are **not** tagged individually. Implement `Comparable<Self>` (see
  [Interfaces § Built-in `Equatable` and `Comparable`](interfaces.md#built-in-equatable-and-comparable))
  instead; all four ordering operators dispatch to its single `compare` method.
- A type may declare multiple binary overloads of the same symbol with different right-hand
  operand types. An exact type match wins over a compatible match; equally good matches are
  ambiguous. Duplicate parameter types are errors, even when the return types differ.
- Unary operators remain unique per symbol, and casts remain unique per target type.
- Equality overloads must return `bool`. `==` and `!=` select the same overload; `!=` negates it.
- Operators dispatch on the left operand. Support both operand orders by declaring an overload
  on each type (including through `extend`); operands are never automatically reversed.
- String concatenation keeps its built-in behavior whenever either operand of `+` is a string.

For example, a type can declare both `fun operator +(other: Vector2): Vector2` and
`fun operator +(other: int): Vector2`. The right operand determines which method is called.

### User-defined casts

`fun implicit(): T` / `fun explicit(): T` on a no-parameter method defines a conversion from the
declaring type to the method's return type:

```dream
class Meters {
    public value: float;

    public constructor(value: float) {
        this.value = value;
    }

    // Explicit only: `(float)meters`, never inferred.
    fun explicit(): float {
        return this.value;
    }
}

class Money {
    public cents: int;

    public constructor(cents: int) {
        this.cents = cents;
    }

    // Implicit: assignable anywhere an `int` is expected, no cast syntax needed.
    fun implicit(): int {
        return this.cents;
    }
}

fun main(): void {
    let m = Meters(2.5);
    let f = (float)m;          // explicit cast required

    let money = Money(150);
    let cents: int = money;    // implicit conversion at a typed `let` binding
}
```

An explicit `(T)expr` cast accepts either `implicit` or `explicit` — implicit
conversions are always also spellable explicitly. Implicit conversions themselves are currently
recognized at typed `let x: T = expr;` bindings; elsewhere, cast explicitly.
