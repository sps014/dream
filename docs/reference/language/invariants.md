# Language rules

Some rules in Dream are permanent by design, not features waiting to be built. The compiler enforces them at parse or analysis time. They are collected here so the resulting errors are never a surprise — code that relies on breaking them is simply not valid Dream.

## Reserved identifiers

Primitive and literal names cannot be reused for a variable, function, parameter, or global:

- Primitives: `int`, `float`, `double`, `string`, `bool`, `char`, `object`, `void`, `long`, `uint`, `ulong`, `byte`.
- Former C#-style spellings (`String`, `Int32`, `Int64`, `UInt32`, `UInt64`, `Byte`, `Single`, `Double`, `Boolean`, `Char`, `Object`, `Void`) are no longer usable as types, but remain reserved as identifiers so old code fails loudly instead of silently shadowing them.
- Literals: `true`, `false`.

The print combinators `__print` / `__println` and any `$`-prefixed name are reserved for the compiler.

```dream
let int = 3;   // error: 'int' is a reserved word
```

## Constructors and destructors

- Extra constructors in a type body are named `constructor`; the destructor is `del`. A primary constructor (`class Point(public x: int, public y: int);`) synthesizes fields and a matching `constructor`.
- `del` may not be `public`/`internal`, and neither `constructor` nor `del` may declare a return type.
- `del` takes no parameters.

Their calling convention is fixed, so their shape is fixed. Call sites are `Type(...)`.

## The object protocol

- `override` applies only to the protocol methods `to_string` and `hash_code`. It must be `public`, take no parameters, and use the fixed return type.
- Any method that overrides a protocol method must be marked `override`.

## Operator overloading

- `fun operator +(...)` only applies to a method; the method's own parameter count (0 or 1)
  fixes whether it overloads the unary or binary form of that symbol.
- `fun implicit(): T` / `fun explicit(): T` only apply to a no-parameter method; its return type is the
  cast's target type.
- A type may declare at most one operator overload per (symbol, arity) and at most one cast
  per target type. See [Operators § Operator overloading](operators.md#operator-overloading).

## Indexers and enumerators

- `fun this[i: int]: T` / `fun this[i: int] = v: T` enable `obj[i]` and `obj[i] = v`.
- Methods named `iterator` (zero parameters, returns a class/struct) and `next` (zero parameters, returns `Option<T>`) enable `for..in`.
- A type may declare at most one method per role. A method named `get`/`set` without `fun this[...]` is ordinary.

## Linkage modifiers are exclusive

`public` and `static` express opposite linkage and cannot combine:

- `public` exposes a symbol to other modules (and, for functions, exports it from the WebAssembly module).
- `static` on a top-level variable pins it to module-internal linkage.

For the same reason, a function cannot be both `public` and `extern` — an `extern` is an imported host symbol, not an exported one.

```dream
public static let x = 1;   // error: cannot be both 'public' and 'static'
```

## Overloading

- Overloads must differ in their parameters; two with identical parameter types are rejected as duplicates.
- Overloads may use default values. An exact-arity match wins over one that fills defaults, and a genuinely ambiguous call is reported at the call site.
- A class/struct's `constructor` may be overloaded exactly like any other method: `Point()`, `Point(x: int, y: int)`, and `Point(both: int)` may all coexist, resolved by the same arity/type rules.

## The entry point

`main` cannot be overloaded and must be declared as `main()` or `main(args: string[])`.

## Control flow

- `break` / `continue` are valid only inside a loop, and any label must resolve to an enclosing loop.
- Assigning to a `const` binding is rejected.

## Top-level globals

Globals initialize in declaration order. An initializer may reference earlier globals but not later ones — there are no forward references at module scope.
