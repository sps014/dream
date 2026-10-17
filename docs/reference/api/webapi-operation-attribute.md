# OperationAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `OperationAttribute`

Describes a route operation in OpenAPI.

```dream
public struct OperationAttribute
```

## `summary`

Short summary displayed in the API documentation.

```dream
public summary: Option<string>
```

## `description`

Description displayed in the API documentation.

```dream
public description: Option<string>
```

## `tags`

Optional tags grouping the operation in the API documentation.

```dream
public tags: Option<string>
```

## `status`

HTTP response status code.

```dream
public status: Option<int>
```

## `deprecated`

Whether the operation is marked deprecated in the API description.

```dream
public deprecated: Option<bool>
```
