# Validation Schema

**Import:** `import system.validation.derive;`

See [model validation](../../web-server/validation.md) for strict decoding, constraints, defaults, and callbacks.

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
