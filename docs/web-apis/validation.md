# Models and validation

Import `system.validation` for strict, reusable model decoding. These same rules describe web request models in OpenAPI.

```dream
import system;
import system.json;
import system.validation;

@json
class Person {
    @min_length(2)
    @max_length(80)
    public name: string;
    @minimum(0.0)
    @maximum(150.0)
    @default_json("18")
    public age: int;
    public constructor(name: string, age: int) {
        this.name = name;
        this.age = age;
    }
}

fun main(): void {
    let person = Validation.decode<Person>("{\"name\":\"Dream\"}").unwrap();
    System.println(person.age);
    let invalid = Validation.validate(Person("x", -1)).unwrap_err();
    System.println(Json.serialize(invalid.to_json()));
}
```

`decode<T>` parses and checks the raw JSON tree before constructing a model. `validate<T>` checks an existing value. Both return `Result<T, ValidationErrors>`. `schema<T>()` returns the generated model rules.

JSON types are strict: `"12"` does not become an integer, and a fractional number cannot fill an integer field. Fields are required unless optional or annotated with `@default_json`. Defaults apply to absence, never to an explicitly invalid value. Unknown properties are rejected; `@allow_extra` on the model permits and ignores them. `@property_name("external_name")` controls the JSON name and error location.

Rules recurse through model fields, arrays, lists, sets, and string-keyed maps. `@minimum`, `@maximum`, `@min_length`, `@max_length`, and `@pattern` express constraints. Validation accumulates independent failures in declaration order, with nested locations containing property names and decimal indices.

For a field callback, use `@validate_with(check)` and define `fun check(value: FieldType): Option<string>`. Return `None` for success or `Some(message)` for failure. For cross-field rules, mark a static method `@model_validator`; it takes the model and returns `List<ValidationIssue>`. Cross-field callbacks run after field validation succeeds. Errors contain locations, codes, and messages, without copies of submitted input.

Use `Json.deserialize` for ordinary JSON decoding without these validation rules. `@default_json` is validation metadata; it does not change an ordinary constructor's defaults.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
