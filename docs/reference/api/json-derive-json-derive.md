# json_derive

**Import:** `import system.json.derive;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `json_derive`

The `@json` derive: `to_json` / `write_json` / `from_json` for every `@json` type, plus the `Json` collection adapters that `Json.serialize/deserialize/from_value` calls on collection types need. One generated file, `json.dream`.

```dream
public fun json_derive(ctx: GenContext): void
```
