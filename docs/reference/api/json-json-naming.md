# JsonNaming

**Import:** `import system.json;`

Read the [usage guide](../../library/serialization/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonNaming`

How `@json` field names are spelled as JSON property names. A field with `@property_name` keeps the name it gives, and map keys and `JsonValue` keys are never renamed.

```dream
public enum JsonNaming
```

## `Identity`

The field name as declared: `user_id` stays `user_id`.

```dream
Identity
```

## `CamelCase`

Lower camel case: `user_id` becomes `userId`.

```dream
CamelCase
```

## `PascalCase`

Upper camel case: `user_id` becomes `UserId`.

```dream
PascalCase
```

## `KebabCase`

Lowercase words joined by `-`: `user_id` becomes `user-id`.

```dream
KebabCase
```

## `SnakeCase`

Lowercase words joined by `_`: `userId` becomes `user_id`.

```dream
SnakeCase
```
