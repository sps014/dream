# JsonGenerator

**Import:** `import system.json;`

Read the [usage guide](../stdlib/json.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class JsonGenerator`

Dream `@json` source generator: builds `extend Type { to_json / write_json / from_json }` source from a [`GenType`] snapshot. Invoked by the compile host during the generate pass.

```dream
public static class JsonGenerator
```

## `expand`

Emits `extend … { to_json / write_json / from_json }` source for each snapshot in `types`.

```dream
public static fun expand(borrow types: List<GenType>, borrow json_names: List<string>, borrow jsonable: List<string>): GenResult
```

## `expand_collections`

```dream
public static fun expand_collections( borrow specs: List<GenCollection>, borrow json_names: List<string>, borrow jsonable: List<string> ): GenResult
```
