# JsonFieldSpec

**Import:** `import system.json.derive;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonFieldSpec`

One serializable field on a class or union variant.

```dream
public class JsonFieldSpec
```

## `name`

Name identifying this entry.

```dream
public name: string
```

## `type_name`

Dream type name represented by this entry.

```dream
public type_name: string
```

## `json_ignore`

Whether this field is omitted from JSON.

```dream
public json_ignore: bool
```

## `property_name`

Name used for this property in serialized data.

```dream
public property_name: string
```

## `option_inner`

Dream type name wrapped by `Option`, when applicable.

```dream
public option_inner: string
```

## `is_type_param`

Whether this reference names a type parameter.

```dream
public is_type_param: bool
```

## `map_value_inner`

Value type of a `Map<string, V>` / `SortedMap<string, V>` field (empty when not a map); the key type is required to be `string` (see `snapshot_field` in `src/driver/generate/json_gen.rs`).

```dream
public map_value_inner: string
```

## `map_ctor`

`Map` or `SortedMap` when `map_value_inner` is set (empty otherwise).

```dream
public map_ctor: string
```

## `seq_elem_inner`

Element type of a `List<T>` / `Set<T>` field (empty when not a seq collection).

```dream
public seq_elem_inner: string
```

## `seq_kind`

`list` or `set` when `seq_elem_inner` is set (empty otherwise).

```dream
public seq_kind: CollectionKind
```

## `constructor`

Builds a field snapshot from SemanticModel data.

```dream
public constructor(
    name: string,
    type_name: string,
    json_ignore: bool,
    property_name: string,
    option_inner: string,
    is_type_param: bool,
    map_value_inner: string,
    map_ctor: string,
    seq_elem_inner: string,
    seq_kind: CollectionKind
)
```

## `make`

Convenience factory matching the constructor arguments.

```dream
public static fun make(
    name: string,
    type_name: string,
    json_ignore: bool,
    property_name: string,
    option_inner: string,
    is_type_param: bool,
    map_value_inner: string,
    map_ctor: string,
    seq_elem_inner: string,
    seq_kind: CollectionKind
): JsonFieldSpec
```

## `json_key`

JSON object key: `@json_property` override, else the Dream field name.

```dream
public fun json_key(): string
```
