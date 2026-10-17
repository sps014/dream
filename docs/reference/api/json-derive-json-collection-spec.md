# JsonCollectionSpec

**Import:** `import system.json.derive;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonCollectionSpec`

Snapshot of a collection type that needs top-level `Json.serialize` / `deserialize` adapters.

```dream
public class JsonCollectionSpec
```

## `kind`

Category of this entry.

```dream
public kind: CollectionKind
```

## `elem_type`

Dream type name of each collection element.

```dream
public elem_type: string
```

## `value_type`

Dream type name of values in this collection.

```dream
public value_type: string
```

## `self_ty`

Dream type name of the collection being described.

```dream
public self_ty: string
```

## `fn_suffix`

Suffix that distinguishes generated helper names.

```dream
public fn_suffix: string
```

## `constructor`

Creates a `JsonCollectionSpec` with `kind`, `elem_type`, `value_type`, `self_ty`, `fn_suffix`.

```dream
public constructor(
    kind: CollectionKind,
    elem_type: string,
    value_type: string,
    self_ty: string,
    fn_suffix: string
)
```

## `make`

Creates a collection description from its kind, element/value types, and generated-name suffix.

```dream
public static fun make(
    kind: CollectionKind,
    elem_type: string,
    value_type: string,
    self_ty: string,
    fn_suffix: string
): JsonCollectionSpec
```
