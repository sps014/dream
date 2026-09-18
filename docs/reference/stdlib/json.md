# JSON

**Import:** `import system.json;` (also loaded when a type has `@json`)

Mark a class `@json` to serialize and parse it. Works on value structs too.

```dream
import system;
import system.json;

@json
class User {
    name: string;
    age: int;
}

fun main() {
    let u = User("Ada", 36);
    let text = Json.serialize(u);
    let back = Json.deserialize<User>(text).unwrap_or(u);
    System.println(back.name);
}
```

| API | Role |
| --- | --- |
| `Json.serialize(x)` | Compact JSON text (`@json` types, `List`/`Set`/`Map<string, V>`/`SortedMap<string, V>`, arrays, and `JsonValue`) |
| `Json.serialize_pretty(value, indent)` | Pretty-printed `JsonValue` (`indent` 0 is compact). For `@json` types use `serialize_pretty(x.to_json(), indent)` |
| `Json.deserialize<T>(text)` | `Result<T, ParseError>` — typed `T`, or `JsonValue` when the shape is unknown |
| `Json.from_value<T>(value)` | Already-parsed `JsonValue` into `T` (`from_value<JsonValue>` is identity) |

Fields may be primitives, `string`, arrays, other `@json` types, tuples (JSON arrays), `List`/`Set`/`Map<string, V>`/`SortedMap<string, V>`, and `Option<T>` of supported types. JSON object keys are strings, so maps must be `Map<string, V>`. `@property_name("key")` renames a JSON key. Skip a field with the ignore attribute. [Unions](../language/enums-unions.md) serialize with a `"type"` tag.

`Json.serialize(data)` infers `T` from `data` (including an annotated `let data: Map<string, string> = …`). `deserialize` still needs an explicit type argument because the JSON text does not name a Dream type.

## Unknown payloads (`JsonValue`)

When a server returns JSON you do not have a class for:

```dream
switch (Json.deserialize<JsonValue>(text)) {
    Ok(v) => {
        System.println(v.get("key").unwrap_or(JsonValue.none()).as_string().unwrap_or(""));
    },
    Err(e) => System.println(e.message()),
}

// HTTP:
switch (r.json()) {
    Ok(v) => System.println(Json.serialize(v)),
    Err(e) => System.println(e.message()),
}
```

Accessors: `as_bool` / `as_int` / `as_double` / `as_string` / `as_array` / `as_map`, `is_null` / `is_array` / `is_object`, `get` / `get_or` / `has` / `set` / `remove` / `keys` on objects, `at` / `push` / `.length` on arrays.

## `GenResult`

Emit-style generators (including `@json` internals) report success or failure with `GenResult.success(source)` / `GenResult.failure(message)`. You rarely construct this by hand — [CodeBuilder](codegen.md) is the usual emit helper.
