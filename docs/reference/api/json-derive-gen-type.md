# JsonTypeSpec

**Import:** `import system.json.derive;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonTypeSpec`

Class or discriminated-union type snapshot for `@json` codegen.

```dream
public class JsonTypeSpec
```

## `name`

Name identifying this entry.

```dream
public name: string
```

## `is_union`

Whether the declaration is a discriminated union.

```dream
public is_union: bool
```

## `generic_params`

Names of the declared type parameters, in order.

```dream
public generic_params: List<string>
```

## `fields`

Fields of this type, in declaration order.

```dream
public fields: List<JsonFieldSpec>
```

## `variants`

Choices declared by this union or enum, in declaration order.

```dream
public variants: List<JsonVariantSpec>
```

## `assign_fields`

Built with a zero-argument constructor plus field stores, because no constructor takes every field in declaration order.

```dream
public assign_fields: bool
```

## `constructor`

Builds a type snapshot (prefer `class_type` / `union_type` factories).

```dream
public constructor(
    name: string,
    is_union: bool,
    generic_params: List<string>,
    fields: List<JsonFieldSpec>,
    variants: List<JsonVariantSpec>
)
```

## `class_type`

Snapshot for a `@json` class/struct with named fields.

```dream
public static fun class_type(
    name: string,
    generic_params: List<string>,
    fields: List<JsonFieldSpec>
): JsonTypeSpec
```

## `union_type`

Snapshot for a `@json` discriminated union with variants.

```dream
public static fun union_type(
    name: string,
    generic_params: List<string>,
    variants: List<JsonVariantSpec>
): JsonTypeSpec
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

## `construct`

Expression building a value from its fields' values (`args`, declaration order).

```dream
public fun construct(args: string): string
```
