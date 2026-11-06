# JsonNull

**Import:** `import system.json;`

Read the [usage guide](../../library/serialization/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsonNull`

What `Json.serialize` writes for an `Option` field of a `@json` type that holds `None`.

```dream
public enum JsonNull
```

## `Include`

Write the property with the value `null`.

```dream
Include
```

## `Omit`

Leave the property out of the object.

```dream
Omit
```
