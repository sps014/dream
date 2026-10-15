# GenAttribute

**Import:** `import system.codegen;`

## `public class GenAttribute`

One attribute use. Prefer the typed `GenAttributes.decode<A>()` (via `attribute<A>()` on a node); the positional readers below are what decoders are built from.

```dream
public class GenAttribute
```

## `public id: string`

`module::name` of the `@attribute` type (`system.json::JsonAttribute`).

```dream
public id: string
```

## `public name: string`

```dream
public name: string
```

## `public args: List<GenAttrArg>`

```dream
public args: List<GenAttrArg>
```

## `public constructor(id: string, name: string, args: List<GenAttrArg>)`

Creates the snapshot model from the supplied values.

```dream
public constructor(id: string, name: string, args: List<GenAttrArg>)
```

## `string_arg`

Returns the requested model information.

```dream
public fun string_arg(i: int): string
```

## `int_arg`

Returns the requested model information.

```dream
public fun int_arg(i: int): int
```

## `long_arg`

Returns the requested model information.

```dream
public fun long_arg(i: int): long
```

## `double_arg`

Returns the requested model information.

```dream
public fun double_arg(i: int): double
```

## `float_arg`

Returns the requested model information.

```dream
public fun float_arg(i: int): float
```

## `bool_arg`

Returns the requested model information.

```dream
public fun bool_arg(i: int): bool
```

## `enum_member`

The member name of an enum argument (`Get` for `HttpMethod.Get`).

```dream
public fun enum_member(i: int): string
```
