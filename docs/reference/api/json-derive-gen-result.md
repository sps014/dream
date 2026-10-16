# JsonGenOutput

**Import:** `import system.json.derive;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonGenOutput`

Outcome of a `@json` source-generator expand pass. Optional `error_type` / `error_field` name the type / field the error is reported at.

```dream
public class JsonGenOutput
```

## `ok`

Whether the operation succeeded.

```dream
public ok: bool
```

## `source`

Generated Dream source text for this JSON result type.

```dream
public source: string
```

## `error`

Error message reported by the operation.

```dream
public error: string
```

## `error_type`

Dream type name of the error payload.

```dream
public error_type: string
```

## `error_field`

Name of the field used to represent the error payload.

```dream
public error_field: string
```

## `success`

Successful expand with emitted Dream source.

```dream
public static fun success(source: string): JsonGenOutput
```

## `failure`

Failed expand with a diagnostic message (no span hint).

```dream
public static fun failure(error: string): JsonGenOutput
```

## `failure_at`

Failed expand with type/field names for host span recovery.

```dream
public static fun failure_at(error: string, type_name: string, field_name: string): JsonGenOutput
```
