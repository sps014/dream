# Validation Schema

**Import:** `import system.validation.derive;`

See [model validation](../../web-apis/validation.md) for strict decoding, constraints, defaults, and callbacks.

## `public static class ValidationGenerator`

Schema generation shared by standalone validation and the web API generator.

```dream
public static class ValidationGenerator
```

## `public static fun constraints`

```dream
public static fun constraints(attributes: List<GenAttribute>, schema: JsonValue): void
```

## `public static fun model`

```dream
public static fun model(d: GenDecl, ctx: GenContext, definitions: JsonValue): JsonValue
```

## `public static fun type_schema`

```dream
public static fun type_schema(ty: GenTypeRef, ctx: GenContext, definitions: JsonValue): JsonValue
```
