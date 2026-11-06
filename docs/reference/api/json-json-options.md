# JsonOptions

**Import:** `import system.json;`

Read the [usage guide](../../library/serialization/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonOptions`

Settings for `Json.serialize`, `Json.deserialize`, and `Json.from_value`. Every setting has a default, so pass only the ones you change: `JsonOptions(naming: JsonNaming.CamelCase)`.

```dream
public class JsonOptions
```

## `naming`

How field names are spelled as property names, in both directions. Default `Identity`.

```dream
public naming: JsonNaming
```

## `nulls`

Whether `None` fields are written as `null` or left out. Reading accepts both. Default `Include`.

```dream
public nulls: JsonNull
```

## `indent`

Spaces per nesting level in written text. `0`, the default, writes compact text on one line; reading ignores this setting.

```dream
public indent: int
```

## `ignore_case`

When true, reading matches property names to fields without regard to ASCII letter case. Writing ignores this setting. Default `false`.

```dream
public ignore_case: bool
```

## `constructor`

Creates options; every parameter defaults to the behavior described on its field.

```dream
public constructor(
    naming: JsonNaming = JsonNaming.Identity,
    nulls: JsonNull = JsonNull.Include,
    indent: int = 0,
    ignore_case: bool = false
)
```
