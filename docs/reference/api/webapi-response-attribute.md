# ResponseAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ResponseAttribute`

Documents an additional explicit response without changing handler behavior.

```dream
public struct ResponseAttribute
```

## `status`

HTTP response status code.

```dream
public status: int
```

## `description`

Description displayed in the API documentation.

```dream
public description: string
```

## `model`

Optional declaration describing the response body.

```dream
public model: Option<DeclarationPath>
```

## `content_type`

Media type of the response body.

```dream
public content_type: Option<string>
```
