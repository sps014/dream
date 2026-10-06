# JsonGenerator

**Import:** `import system.json.derive;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class JsonGenerator`

Builds `extend Type { to_json / write_json / from_json }` source from [`JsonTypeSpec`]s (see `json_derive`).

```dream
public static class JsonGenerator
```

## `expand`

Emits `extend … { to_json / write_json / from_json }` source for each snapshot in `types`.

```dream
public static fun expand(borrow types: List<JsonTypeSpec>, borrow json_names: List<string>, borrow jsonable: List<string>): JsonGenOutput
```

## `expand_collections`

Emits free `__col_ser_*` / `__col_write_*` / `__col_de_*` helpers for top-level collection `Json.serialize` / `deserialize` / `from_value` (see `intrinsics.rs`).

```dream
public static fun expand_collections( borrow specs: List<JsonCollectionSpec>, borrow json_names: List<string>, borrow jsonable: List<string> ): JsonGenOutput
```
