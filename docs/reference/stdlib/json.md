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

- `Json.serialize(x)` → string
- `Json.deserialize<T>(text)` → `Result<T, ParseError>`
- `Json.from_value<T>(value)` — from an already-parsed `JsonValue`

Fields may be primitives, `string`, arrays, other `@json` types, tuples (JSON arrays), and `Option<T>` of supported types. `@property_name("key")` renames a JSON key. Skip a field with the ignore attribute. [Unions](../language/enums-unions.md) serialize with a `"type"` tag.

## Untyped `JsonValue`

`Json.parse` / `Json.stringify` / `Json.stringify_pretty` when you do not have a class.

Accessors: `as_bool` / `as_int` / `as_double` / `as_string` / `as_array` / `as_map`, `is_null` / `is_array` / `is_object`, `get` / `get_or` / `has` / `set` / `remove` / `keys` on objects, `at` / `push` / `.length` on arrays.

## `GenResult`

Emit-style generators (including `@json` internals) report success or failure with `GenResult.success(source)` / `GenResult.failure(message)`. You rarely construct this by hand — [CodeBuilder](codegen.md) is the usual emit helper.
