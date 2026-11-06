---
title: JSON
summary: "Parse JSON, and derive encoders for your types with `@json`."
icon: material/code-json
api: reference/api/system-json.md
---

# JSON


JSON lets your program exchange structured data as text. Mark your own data type with `@json` for convenient conversion, or use JsonValue when the data's structure is not known in advance.

**Import:** `import system.json;` (also loaded when a type has `@json`)

Mark a class `@json` to serialize and parse it. Works on value structs too.

```dream
import system;
import system.json;

@json
class User {
    public name: string;
    public age: int;
}

fun main() {
    let u = User();
    u.name = "Ada";
    u.age = 36;
    let text = Json.serialize(u);
    let back = Json.deserialize<User>(text).unwrap_or(u);
    System.println(back.name);
}
```

A `@json` class with no constructor of its own uses the implicit zero-argument constructor. `Json.deserialize` fills the fields after that. Declare a constructor that takes the fields in order when you want `User("Ada", 36)`.

| API | Role |
| --- | --- |
| `Json.serialize(x)` | Compact JSON text (`@json` types, `List`/`Set`/`Map<string, V>`/`SortedMap<string, V>`, arrays, and `JsonValue`) |
| `Json.serialize(x, options)` | JSON text with the naming, `null`, and indentation settings in `options` |
| `Json.deserialize<T>(text)` | `Result<T, ParseError>` — typed `T`, or `JsonValue` when the shape is unknown |
| `Json.deserialize<T>(text, options)` | Same, matching property names to fields as `options` describes |
| `Json.from_value<T>(value)` | Already-parsed `JsonValue` into `T` (`from_value<JsonValue>` is identity) |
| `Json.from_value<T>(value, options)` | Same, matching property names to fields as `options` describes |

Fields may be primitives, `string`, arrays, other `@json` types, tuples (JSON arrays), `List`/`Set`/`Map<string, V>`/`SortedMap<string, V>`, `JsonValue`, and `Option<T>` of supported types. JSON object keys are strings, so maps must be `Map<string, V>`. `@property_name("key")` renames a JSON key. Skip a field with the ignore attribute. [Unions](../../language/types/enums-unions.md) serialize with a `"type"` tag.

Every field type must be one `@json` already knows. `object` is not: `Json.serialize` on a `Map<string, object>` is rejected. Reach for `JsonValue` instead — it carries its own shape and is passed through untouched.

`Json.serialize(data)` infers `T` from `data` (including an annotated `let data: Map<string, string> = …`). `deserialize` still needs an explicit type argument because the JSON text does not name a Dream type.

## Options

Pass a `JsonOptions` to change how property names are spelled, whether `None` fields are written, and whether the text is indented. Each setting has a default, so name only the ones you change:

```dream
import system;
import system.json;

@json
class Account {
    public user_id: int;
    public display_name: string;
    public nick_name: Option<string>;

    public constructor(user_id: int, display_name: string, nick_name: Option<string>) {
        this.user_id = user_id;
        this.display_name = display_name;
        this.nick_name = nick_name;
    }
}

fun main() {
    let none: Option<string> = Option.None;
    let account = Account(7, "Ada", none);
    let options = JsonOptions(naming: JsonNaming.CamelCase, nulls: JsonNull.Omit, indent: 2);

    let text = Json.serialize(account, options);
    System.println(text);

    let back = Json.deserialize<Account>(text, options).unwrap_or(account);
    System.println(back.display_name);
}
```

The program prints:

```text
{
  "userId": 7,
  "displayName": "Ada"
}
Ada
```

| Setting | Default | Effect |
| --- | --- | --- |
| `naming` | `JsonNaming.Identity` | Spells field names as declared, or as `CamelCase` (`userId`), `PascalCase` (`UserId`), `KebabCase` (`user-id`), or `SnakeCase` (`user_id`). Applies when writing and reading. |
| `nulls` | `JsonNull.Include` | `Include` writes `None` fields as `null`; `Omit` leaves them out. Reading accepts both. |
| `indent` | `0` | Spaces per nesting level. `0` writes compact text on one line. |
| `ignore_case` | `false` | When `true`, reading matches property names to fields regardless of ASCII letter case. |

Naming changes only the names of declared fields. A `@property_name("key")` field always uses its key exactly, and map keys and the keys inside a `JsonValue` are written as they are. Read with the same `naming` you wrote with: a camel-case reader does not match `user_id` and leaves that field at its default. Add `ignore_case: true` to accept keys such as `USERID` or `DisplayName` from other systems.

Indentation applies to every value `Json.serialize` accepts, including lists and `JsonValue` trees. Empty objects and arrays stay `{}` and `[]`.

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

`v.get(key)` returns the property as `Option<JsonValue>`: `None` when the key is missing or `v` is not an object. Chain a typed reader through `and_then` to decode it, and `unwrap_or` to supply a fallback; a wrong JSON type is also `None`. `v[key]` returns the property directly and panics when it is missing, so use it only when the shape is already known. `v.contains(key)` tests presence.

```dream
let name = v.get("name").and_then((item: JsonValue) => item.as_string()).unwrap_or("");
let n = v.get("count").and_then((item: JsonValue) => item.as_int()).unwrap_or(0);
if v.contains("error") {
    System.println(v.get("error").and_then((item: JsonValue) => item.as_string()).unwrap_or(""));
}
```

`is_*` / `kind()` are an integer tag check. `as_bool` / `as_int` / `as_double` wrap a scalar in `Option`. `as_string` clones the string. `as_map` / `as_array` return the existing container.

### Walk with `kind()`

```dream
switch (v.kind()) {
    case JsonKind.Object:
        let i = 0;
        while i < v.length { // insertion order
        let key = v.key_at(i).unwrap_or("");
        let child = v.value_at(i).unwrap_or(JsonValue.null());
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
        System.println(meta.get("id").and_then((item: JsonValue) => item.as_int()).unwrap_or(0));
    },
    None => {}
}
```

### Known shape with one open field

```dream
@json
class Envelope {
    public id: int;
    public payload: JsonValue; // kept verbatim through serialize/deserialize
}

let mixed: Map<string, JsonValue> = {
    "n": JsonValue.from_int(42), "s": JsonValue.from_string("text"),
};
System.println(Json.serialize(mixed)); // {"n":42,"s":"text"}
```

### Accessors

| Kind | Test | Read | Build |
| --- | --- | --- | --- |
| object `{...}` | `kind()` / `is_object` | `get_str` / `get_int` / `get` / `has` / `key_at` / `as_map` | `JsonValue.dict()`, then `set` |
| array `[...]` | `kind()` / `is_array` | `at(index)` / `as_array` / `.length` | `JsonValue.array()`, then `push` |
| string / number / bool / null | `kind()` / `is_null` | `str_or` / `int_or` / `as_string` / `as_int` / `as_double` / `as_bool` | `from_string` / `from_int` / `number` / `boolean` / `none` |

## How `@json` works

`@json` is an ordinary Dream source generator, `json_derive` in `system.json`. Importing `system.json` registers it, so it works the same in single files, packages, dependencies, `dream test`, and wasm builds. It runs only when the program has a `@json` type or calls `Json.serialize`, `Json.deserialize`, or `Json.from_value`. It is `@incremental`, so unchanged programs replay its cached output. The generated code is written to `generated/<entry>/system/json/derive/json_derive/json.dream`; to step through the derive itself, see [Debugging a generator](../../language/metaprogramming/generators.md#debugging-a-generator).

