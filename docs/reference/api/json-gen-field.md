# GenField

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GenField`

One serializable field on a class or union variant.

```dream
public class GenField
```

## `name: string`

```dream
public name: string
```

## `type_name: string`

```dream
public type_name: string
```

## `json_ignore: bool`

```dream
public json_ignore: bool
```

## `property_name: string`

```dream
public property_name: string
```

## `option_inner: string`

```dream
public option_inner: string
```

## `is_type_param: bool`

```dream
public is_type_param: bool
```

## `map_value_inner: string`

```dream
public map_value_inner: string
```

## `map_ctor: string`

`Map` or `SortedMap` when `map_value_inner` is set (empty otherwise).

```dream
public map_ctor: string
```

## `seq_elem_inner: string`

Element type of a `List<T>` / `Set<T>` field (empty when not a seq collection).

```dream
public seq_elem_inner: string
```

## `seq_kind: string`

`list` or `set` when `seq_elem_inner` is set (empty otherwise).

```dream
public seq_kind: string
```

## `constructor`

Builds a field snapshot from SemanticModel data.

```dream
public constructor( name: string, type_name: string, json_ignore: bool, property_name: string, option_inner: string, is_type_param: bool, map_value_inner: string, map_ctor: string, seq_elem_inner: string, seq_kind: string )
```

## `make`

Convenience factory matching the constructor arguments.

```dream
public static fun make( name: string, type_name: string, json_ignore: bool, property_name: string, option_inner: string, is_type_param: bool, map_value_inner: string, map_ctor: string, seq_elem_inner: string, seq_kind: string ): GenField
```

## `json_key`

JSON object key: `@json_property` override, else the Dream field name.

```dream
public fun json_key(): string
```
