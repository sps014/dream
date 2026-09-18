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

Encoding is decided at compile time, so every value type must be one the generator recognizes. `object` is not: `Json.serialize` on a `Map<string, object>` is rejected. Reach for `JsonValue` instead — it carries its own shape and is passed through untouched.

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

Every `as_*` returns `Option`: missing keys and wrong types become `None`, they do not panic. Prefer `get` over `at` (`at` panics if the key is missing).

```dream
let name = v.get("name").unwrap_or(JsonValue.none()).as_string().unwrap_or("");
let n = v.get("count").unwrap_or(JsonValue.none()).as_int().unwrap_or(0);
if v.has("error") {
    System.println(v.get_or("error", JsonValue.none()).as_string().unwrap_or(""));
}
```

### Walk an object (the dict)

`is_object()` is true for `{...}`. Keys stay in insertion order. Values are still `JsonValue`, so nested objects and arrays walk the same way.

```dream
if v.is_object() {
    let i = 0;
    while i < v.length {
        let key = v.key_at(i).unwrap_or("");
        let child = v.value_at(i).unwrap_or(JsonValue.none());
        System.println(key + " = " + Json.serialize(child));
        i = i + 1;
    }
}

// Same data as a Map, if you want Map APIs:
let dict: Map<string, JsonValue> = v.as_map().unwrap_or(Map<string, JsonValue>());
```

`Json.serialize(dict)` round-trips that map.

### Walk an array

```dream
if v.is_array() {
    let i = 0;
    while i < v.length {
        let item = v.at(i).unwrap_or(JsonValue.none());
        System.println(Json.serialize(item));
        i = i + 1;
    }
}
```

### Nested values

```dream
let nested = v.get("meta").unwrap_or(JsonValue.none());
if nested.is_object() {
    System.println(nested.get("id").unwrap_or(JsonValue.none()).as_int().unwrap_or(0));
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
| object `{...}` | `is_object` | `get` / `get_or` / `has` / `keys` / `key_at` / `value_at` / `as_map` | `JsonValue.dict()`, then `set` |
| array `[...]` | `is_array` | `at(index)` / `as_array` / `.length` | `JsonValue.array()`, then `push` |
| string / number / bool / null | `is_null` | `as_string` / `as_int` / `as_double` / `as_bool` | `from_string` / `from_int` / `number` / `boolean` / `none` |

## `GenResult`

Emit-style generators (including `@json` internals) report success or failure with `GenResult.success(source)` / `GenResult.failure(message)`. You rarely construct this by hand — [CodeBuilder](codegen.md) is the usual emit helper.
