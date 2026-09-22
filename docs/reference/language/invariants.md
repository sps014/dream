# Language rules

These rules are part of Dream. Breaking one is an error.

## Reserved identifiers

These names cannot be reused for a variable, function, parameter, or global:

- Primitives: `int`, `float`, `double`, `string`, `bool`, `char`, `object`, `void`, `long`, `uint`, `ulong`, `byte`.
- These are reserved names, not types: `String`, `Int32`, `Int64`, `UInt32`, `UInt64`, `Byte`, `Single`, `Double`, `Boolean`, `Char`, `Object`, `Void`.
- Literals: `true`, `false`.
- `__print`, `__println`, and any `$`-prefixed name — you cannot use these names.

```dream
let int = 3;   // error: 'int' is a reserved word
```

## Constructors and destructors

- Extra constructors in a type body are named `constructor`; the destructor is `del`. A primary constructor (`class Point(x: int, y: int);`) synthesizes public fields and a matching `constructor`.
- `del` may not be `public` / `internal`, and neither `constructor` nor `del` may declare a return type.
- `del` takes no parameters.

```dream
// good
class Box {
    public constructor() { }
    del() { }
}

// bad — del cannot be public; constructor cannot declare a return type
class Bad {
    public del() { }
    public constructor(): void { }
}
```

Call sites are `Type(...)`.

## The object protocol

- `override` applies only to the protocol methods `to_string` and `hash_code`. It must be `public`, take no parameters, and use the fixed return type.
- Any method that overrides a protocol method must be marked `override`.

```dream
// good
@override public fun to_string(): string { return "x"; }

// bad — missing @override, or not public
public fun to_string(): string { return "x"; }
```

## Operator overloading

- `fun operator +(...)` only applies to a method; the method's own parameter count (0 or 1) fixes whether it overloads the unary or binary form of that symbol.
- `fun implicit(): T` / `fun explicit(): T` only apply to a no-parameter method; its return type is the cast's target type.
- A type may declare at most one operator overload per (symbol, arity) and at most one cast per target type. See [Operators § Operator overloading](operators.md#operator-overloading).

## Indexers and enumerators

- `fun this[i: int]: T` / `fun this[i: int] = v: T` enable `obj[i]` and `obj[i] = v`.
- Methods named `iterator` (zero parameters, returns a class/struct) and `next` (zero parameters, returns `Option<T>`) enable `for..in`.
- A type may declare at most one method per role. A method named `get` / `set` without `fun this[...]` is ordinary.

## Linkage modifiers are exclusive

`public` exposes a symbol to other modules (and, for functions, to the host that runs the program). A function therefore cannot be both `public` and `extern` — an `extern` is an imported host symbol, not an exported one.

```dream
// bad — public and extern are exclusive
public extern fun host_call(): void;
```

`static` declares class members, never top-level variables; a top-level `let` is file-private unless marked `internal` / `public`:

```dream
static let x = 1;   // error: 'static' cannot modify a top-level variable
```

## Overloading

- Overloads must differ in their parameters; two with identical parameter types are rejected as duplicates.
- Overloads may use default values. An exact-arity match wins over one that fills defaults, and a genuinely ambiguous call is reported at the call site.
- A class/struct's `constructor` may be overloaded exactly like any other method: `Point()`, `Point(x: int, y: int)`, and `Point(both: int)` may all coexist, resolved by the same arity/type rules.

## The entry point

`main` cannot be overloaded and must be declared as `main()` or `main(args: string[])`.

```dream
// good shapes (pick one)
fun main(): void { }
// or:
fun main(args: string[]): void { }

// bad — wrong signature
fun main(n: int): void { }
```

## Control flow

- `break` / `continue` are valid only inside a loop, and any label must resolve to an enclosing loop.
- Assigning to a `const` binding is rejected.

```dream
const n = 1;
n = 2;   // error: cannot assign to const
```

## Top-level globals

Globals initialize in declaration order. An initializer may reference earlier globals but not later ones — there are no forward references at module scope.

```dream
let a = 1;
let b = a + 1;   // good
let c = d + 1;   // error: forward reference
let d = 2;
```
