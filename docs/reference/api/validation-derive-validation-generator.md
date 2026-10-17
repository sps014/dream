# ValidationGenerator

**Import:** `import system.validation.derive;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ValidationGenerator`

Schema generation shared by standalone validation and the web API generator.

```dream
public static class ValidationGenerator
```

## `constraints`

Adds the constraints described by `attributes` to the JSON schema.

```dream
public static fun constraints(attributes: List<GenAttribute>, schema: JsonValue): void
```

## `model`

Builds a JSON schema for declaration `d`, recording reusable definitions.

```dream
public static fun model(d: GenDecl, ctx: GenContext, definitions: JsonValue): JsonValue
```

## `type_schema`

Builds a JSON schema for `ty`, recording reusable definitions.

```dream
public static fun type_schema(ty: GenTypeRef, ctx: GenContext, definitions: JsonValue): JsonValue
```
