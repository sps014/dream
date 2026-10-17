# JsonVariantSpec

**Import:** `import system.json.derive;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonVariantSpec`

One variant of a `@json` discriminated union.

```dream
public class JsonVariantSpec
```

## `name`

Name identifying this entry.

```dream
public name: string
```

## `fields`

Fields of this type, in declaration order.

```dream
public fields: List<JsonFieldSpec>
```

## `constructor`

Builds a variant spec.

```dream
public constructor(name: string, fields: List<JsonFieldSpec>)
```

## `make`

Convenience factory matching the constructor arguments.

```dream
public static fun make(name: string, fields: List<JsonFieldSpec>): JsonVariantSpec
```
