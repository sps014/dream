# Json Generator

**Import:** `import system.json.derive;`

## `JsonGenerator`

Builds `extend Type { to_json / write_json / from_json }` source from [`JsonTypeSpec`]s (see `json_derive`).

```dream
public static class JsonGenerator
```

## `expand`

Emits `extend … { to_json / write_json / from_json }` source for each snapshot in `types`.

```dream
public static fun expand(
    borrow types: List<JsonTypeSpec>,
    borrow json_names: List<string>,
    borrow jsonable: List<string>
): JsonGenOutput
```

## `members`

Emits only methods for GenContext.emit_extend, retaining the model's declaration scope.

```dream
public static fun members(
    borrow t: JsonTypeSpec,
    borrow json_names: List<string>,
    borrow jsonable: List<string>
): JsonGenOutput
```

## `expand_collections`

Generates JSON serialization and deserialization support for the supplied collection descriptions.

```dream
public static fun expand_collections(
    borrow specs: List<JsonCollectionSpec>,
    borrow json_names: List<string>,
    borrow jsonable: List<string>
): JsonGenOutput
```
