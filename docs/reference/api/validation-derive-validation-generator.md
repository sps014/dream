# Validation Generator

**Import:** `import system.validation.derive;`

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
public static fun type_schema( ty: GenTypeRef, ctx: GenContext, definitions: JsonValue ): JsonValue
```
