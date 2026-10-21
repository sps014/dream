# GenAttribute

**Import:** `import system.codegen;`

## `GenAttribute`

One attribute use. Prefer the typed `GenAttributes.decode<A>()` (via `attribute<A>()` on a node); the positional readers below are what decoders are built from.

```dream
public class GenAttribute
```

## `id`

`module::name` of the `@attribute` type (`system.json::JsonAttribute`).

```dream
public id: string
```

## `name`

Unqualified attribute name as written, without the `@` (`json`).

```dream
public name: string
```

## `args`

Arguments supplied to the attribute, in declaration order.

```dream
public args: List<GenAttrArg>
```

## `constructor`

Creates a `GenAttribute` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(id: string, name: string, args: List<GenAttrArg>)
```

## `string_arg`

Argument `i` as unquoted text, or an empty string when there is no such argument.

```dream
public fun string_arg(i: int): string
```

## `int_arg`

Argument `i` parsed as an `int`, or `0` when it is missing or not an integer.

```dream
public fun int_arg(i: int): int
```

## `long_arg`

Argument `i` parsed as a `long`, or `0` when it is missing or not an integer.

```dream
public fun long_arg(i: int): long
```

## `double_arg`

Argument `i` parsed as a `double`, or `0.0` when it is missing or not a number.

```dream
public fun double_arg(i: int): double
```

## `float_arg`

Argument `i` parsed as a `float`, or `0.0` when it is missing or not a number.

```dream
public fun float_arg(i: int): float
```

## `bool_arg`

Whether argument `i` is the literal `true`; missing arguments read as `false`.

```dream
public fun bool_arg(i: int): bool
```

## `enum_member`

The member name of an enum argument (`Get` for `HttpMethod.Get`).

```dream
public fun enum_member(i: int): string
```
