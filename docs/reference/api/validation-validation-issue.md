# ValidationIssue

**Import:** `import system.validation;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ValidationIssue`

One input failure; locations use JSON property names and decimal collection indices.

```dream
public class ValidationIssue
```

## `location`

Path to the invalid value, from the outermost field to the failing item.

```dream
public location: List<string>
```

## `code`

Error code identifying the failure.

```dream
public code: string
```

## `message`

Human-readable explanation of the result or failure.

```dream
public message: string
```

## `constructor`

Creates a `ValidationIssue` with `location`, `code`, `message`.

```dream
public constructor(location: List<string>, code: string, message: string)
```
