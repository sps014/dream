# GenType

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GenType`

```dream
public class GenType
```

## `name: string`

```dream
public name: string
```

## `is_union: bool`

```dream
public is_union: bool
```

## `generic_params: List<string>`

```dream
public generic_params: List<string>
```

## `fields: List<GenField>`

```dream
public fields: List<GenField>
```

## `variants: List<GenVariant>`

```dream
public variants: List<GenVariant>
```

## `constructor`

Builds a type snapshot (prefer `class_type` / `union_type` factories).

```dream
public constructor( name: string, is_union: bool, generic_params: List<string>, fields: List<GenField>, variants: List<GenVariant> )
```

## `class_type`

Snapshot for a `@json` class/struct with named fields.

```dream
public static fun class_type( name: string, generic_params: List<string>, fields: List<GenField> ): GenType
```

## `union_type`

Snapshot for a `@json` discriminated union with variants.

```dream
public static fun union_type( name: string, generic_params: List<string>, variants: List<GenVariant> ): GenType
```

## `params_clause`

`<T, U>` clause for emit, or empty when non-generic.

```dream
public fun params_clause(): string
```

## `self_ty`

Fully written self type (`Name` or `Name<T, U>`).

```dream
public fun self_ty(): string
```
