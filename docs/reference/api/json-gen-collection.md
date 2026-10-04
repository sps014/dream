# GenCollection

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GenCollection`

Snapshot of a collection type that needs top-level `Json.serialize` / `deserialize` adapters.

```dream
public class GenCollection
```

## `kind: string`

```dream
public kind: string
```

## `elem_type: string`

```dream
public elem_type: string
```

## `value_type: string`

```dream
public value_type: string
```

## `self_ty: string`

```dream
public self_ty: string
```

## `fn_suffix: string`

```dream
public fn_suffix: string
```

## `constructor`

```dream
public constructor( kind: string, elem_type: string, value_type: string, self_ty: string, fn_suffix: string )
```

## `make`

```dream
public static fun make( kind: string, elem_type: string, value_type: string, self_ty: string, fn_suffix: string ): GenCollection
```
