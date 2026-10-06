# JsonVariantSpec

**Import:** `import system.json.derive;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class JsonVariantSpec`

One variant of a `@json` discriminated union.

```dream
public class JsonVariantSpec
```

## `name: string`

```dream
public name: string
```

## `fields: List<JsonFieldSpec>`

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
