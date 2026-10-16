# system.json: Attribute declarations

Import `system.json` to use these declarations.

## `JsonAttribute`

Derives JSON serialization for a class, struct or discriminated union: `to_json`, `write_json` and `from_json`, written by the `system.json.derive` generator.

```dream
public struct JsonAttribute
```

## `PropertyNameAttribute`

Overrides the JSON property name of a field of a `@json` type.

```dream
public struct PropertyNameAttribute
```

## `name`

Name identifying this entry.

```dream
public name: string
```

## `JsonIgnoreAttribute`

Leaves a field of a `@json` type out of JSON serialization and deserialization.

```dream
public struct JsonIgnoreAttribute
```
