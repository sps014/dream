# json, property_name, json_ignore

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct json`

Derives JSON serialization for a class, struct or discriminated union: `to_json`, `write_json` and `from_json`, written by the `system.json.derive` generator.

```dream
public struct json
```

## `struct property_name`

Overrides the JSON property name of a field of a `@json` type.

```dream
public struct property_name
```

## `name: string`

```dream
public name: string
```

## `struct json_ignore`

Leaves a field of a `@json` type out of JSON serialization and deserialization.

```dream
public struct json_ignore
```
