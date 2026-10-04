# GenVariant

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GenVariant`

One variant of a `@json` discriminated union.

```dream
public class GenVariant
```

## `name: string`

```dream
public name: string
```

## `fields: List<GenField>`

```dream
public fields: List<GenField>
```

## `constructor`

Builds a variant snapshot from SemanticModel data.

```dream
public constructor(name: string, fields: List<GenField>)
```

## `make`

Convenience factory matching the constructor arguments.

```dream
public static fun make(name: string, fields: List<GenField>): GenVariant
```
