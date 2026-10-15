# system.json: Attribute declarations

Import `system.json` to use these declarations.

```dream
public struct JsonAttribute
```

Derives JSON serialization for a class, struct or discriminated union: `to_json`, `write_json` and `from_json`, written by the `system.json.derive` generator.

```dream
public struct PropertyNameAttribute
```

Overrides the JSON property name of a field of a `@json` type.

```dream
public name: string
```

```dream
public struct JsonIgnoreAttribute
```

Leaves a field of a `@json` type out of JSON serialization and deserialization.
