# Json Generator

**Import:** `import system.json.derive;`

## `public static class JsonGenerator`

Builds `extend Type { to_json / write_json / from_json }` source from [`JsonTypeSpec`]s (see `json_derive`).

```dream
public static class JsonGenerator
```

## `public static fun expand`

Emits `extend … { to_json / write_json / from_json }` source for each snapshot in `types`.

```dream
public static fun expand(borrow types: List<JsonTypeSpec>, borrow json_names: List<string>, borrow jsonable: List<string>): JsonGenOutput
```

## `public static fun members`

Emits only methods for GenContext.emit_extend, retaining the model's declaration scope.

```dream
public static fun members(borrow t: JsonTypeSpec, borrow json_names: List<string>, borrow jsonable: List<string>): JsonGenOutput
```

## `public static fun expand_collections`

Emits free `__col_ser_*` / `__col_write_*` / `__col_de_*` helpers for top-level collection `Json.serialize` / `deserialize` / `from_value` (see `intrinsics.rs`).

```dream
public static fun expand_collections( borrow specs: List<JsonCollectionSpec>, borrow json_names: List<string>, borrow jsonable: List<string> ): JsonGenOutput
```
