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

Fields may be primitives, `string`, arrays, other `@json` types, tuples (JSON arrays), `List`/`Set`/`Map<string, V>`/`SortedMap<string, V>`, `JsonValue`, and `Option<T>` of supported types. JSON object keys are strings, so maps must be `Map<string, V>`. `@property_name("key")` renames a JSON key. Skip a field with the ignore attribute. [Unions](../language/enums-unions.md) serialize with a `"type"` tag.

Every field type must be one `@json` already knows. `object` is not: `Json.serialize` on a `Map<string, object>` is rejected. Reach for `JsonValue` instead — it carries its own shape and is passed through untouched.

`Json.serialize(data)` infers `T` from `data` (including an annotated `let data: Map<string, string> = …`). `deserialize` still needs an explicit type argument because the JSON text does not name a Dream type.

## Unknown payloads (`JsonValue`)

When you do not have a class for the JSON (an API body, a config blob, mixed values), parse to `JsonValue`. That is the string-keyed dict of mixed values — **not** `Map<string, object>`, which cannot be serialized.

### Parse

```dream
switch (Json.deserialize<JsonValue>(text)) {
    Ok(v) => walk(v),
    Err(e) => System.println(e.message()),
}

// HTTP already returns a tree:
switch (r.json()) {
    Ok(v) => walk(v),
    Err(e) => System.println(e.message()),
}
```

### Read one field

`get_str` / `get_int` / `get_bool` look up a key and decode it. Missing keys and wrong types are `None`. `get_str_or` / `get_int_or` supply a fallback. Prefer these over `get` + `unwrap_or(JsonValue.none())` + `as_*` — that path allocates a dummy node on every miss. `at(key)` panics if the key is absent.

```dream
let name = v.get_str_or("name", "");
let n = v.get_int_or("count", 0);
if v.has("error") {
    System.println(v.get_str_or("error", ""));
}
```

`is_*` / `kind()` are an integer tag check. `as_bool` / `as_int` / `as_double` wrap a scalar in `Option`. `as_string` clones the string. `as_map` / `as_array` return the existing container.

### Walk with `kind()`

```dream
switch (v.kind()) {
    case JsonKind.Object:
        let i = 0;
        while i < v.length {                 // insertion order
            let key = v.key_at(i).unwrap_or("");
            let child = v.value_at(i).unwrap_or(JsonValue.none());
            System.println(key + " = " + Json.serialize(child));
            i = i + 1;
        }
    case JsonKind.Array:
        switch (v.as_array()) {
            Some(items) => {
                for (let item in items) {
                    System.println(Json.serialize(item));
                }
            },
            None => {}
        }
    default:
}

let dict: Map<string, JsonValue> = v.as_map().unwrap_or(Map<string, JsonValue>());
```

`Json.serialize(dict)` round-trips that map. Iterating `as_map()` is unordered; `key_at` / `value_at` follow JSON key order.

### Nested values

```dream
switch (v.get("meta")) {
    Some(meta) => {
        System.println(meta.get_int_or("id", 0));
    },
    None => {}
}
```

### Known shape with one open field

```dream
@json
class Envelope {
    public id: int;
    public payload: JsonValue;   // kept verbatim through serialize/deserialize
}

let mixed: Map<string, JsonValue> = {
    "n": JsonValue.from_int(42),
    "s": JsonValue.from_string("text"),
};
System.println(Json.serialize(mixed));   // {"n":42,"s":"text"}
```

### Accessors

| Kind | Test | Read | Build |
| --- | --- | --- | --- |
| object `{...}` | `kind()` / `is_object` | `get_str` / `get_int` / `get` / `has` / `key_at` / `as_map` | `JsonValue.dict()`, then `set` |
| array `[...]` | `kind()` / `is_array` | `at(index)` / `as_array` / `.length` | `JsonValue.array()`, then `push` |
| string / number / bool / null | `kind()` / `is_null` | `str_or` / `int_or` / `as_string` / `as_int` / `as_double` / `as_bool` | `from_string` / `from_int` / `number` / `boolean` / `none` |

## `GenResult`

`GenResult.success(source)` / `GenResult.failure(message)` report emit-style generator outcomes; you rarely construct this by hand — prefer [CodeBuilder](codegen.md).
