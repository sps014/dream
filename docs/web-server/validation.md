---
title: Models and validation
summary: Decode request models strictly and reuse the rules in OpenAPI.
icon: material/check-all
---

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

The first line prints `18`, because the absent `age` takes its default. The second prints the two failures found in `Person("x", -1)`: `name` is too short and `age` is below the minimum.

## Constraint attributes

Put these on fields of an `@json` model, or on handler parameters such as `@query` and `@header` values. Arguments are positional.

| Attribute | Use on | Fails when | Error code |
| --- | --- | --- | --- |
| `@minimum(value)` | Numbers | The value is less than `value` (inclusive bound) | `minimum` |
| `@maximum(value)` | Numbers | The value is greater than `value` (inclusive bound) | `maximum` |
| `@min_length(n)` | Strings, arrays, `List`, `Set`, `Map` | The string length, element count, or entry count is below `n` | `min_length` |
| `@max_length(n)` | Strings, arrays, `List`, `Set`, `Map` | The string length, element count, or entry count is above `n` | `max_length` |
| `@pattern(regex)` | Strings | The string does not match the regular expression | `pattern` |
| `@default_json(json)` | Any field or parameter | Never; supplies a value when the input is absent | — |
| `@validate_with(check)` | Fields | `check(value)` returns `Some(message)` | `custom` |

`@minimum` and `@maximum` take a `double`, so write `@minimum(0.0)` even on an `int` field. `@default_json` takes JSON text: `@default_json("18")` for a number, `@default_json("\"guest\"")` for a string. Invalid JSON is a compile-time error.

On an `Option<T>` field, constraints apply only when a value is present.

## Model attributes

| Attribute | Put on | Effect |
| --- | --- | --- |
| `@json` | Class or struct | Required. Generates decoding, encoding, and the validation schema. |
| `@allow_extra` | Class or struct | Accept and ignore unknown properties instead of failing with `extra`. |
| `@property_name("name")` | Field | Use `name` as the JSON property and in error locations. |
| `@json_ignore` | Field | Leave the field out of JSON and skip its validation. |
| `@model_validator` | Static method | Run a cross-field check after every field passes. |

## Validation functions

| Function | Returns | Use it to |
| --- | --- | --- |
| `Validation.decode<T>(text)` | `Result<T, ValidationErrors>` | Parse JSON text, apply defaults, check every rule, then construct `T`. |
| `Validation.validate<T>(value)` | `Result<T, ValidationErrors>` | Check a value you already have. |
| `Validation.schema<T>()` | `JsonValue` | Get the JSON Schema for `T`, including its constraints. |

`ValidationErrors.issues` is a `List<ValidationIssue>`. Each issue has a `location` (property names and decimal indices from the outermost field inward), a `code`, and a `message`. `to_json()` produces the `{"detail": [{"loc", "type", "msg"}]}` shape that web handlers return.

## Error codes

| Code | Meaning |
| --- | --- |
| `json` | The text is not valid JSON. |
| `type` | The JSON type does not match the field, such as a string for an `int`. |
| `missing` | A required property is absent. |
| `extra` | An unknown property is present and the model has no `@allow_extra`. |
| `minimum`, `maximum` | A number is outside its bounds. |
| `min_length`, `max_length` | A string, collection, or map is too short or too long. |
| `pattern` | A string does not match its `@pattern`. |
| `custom` | A `@validate_with` function returned a message. |

A `@model_validator` returns `ValidationIssue` values with codes you choose.

## Decoding rules

JSON types are strict: `"12"` does not become an integer, and a fractional number cannot fill an integer field. Integer fields also check their range, so `3000000000` fails for an `int`. Fields are required unless they are `Option<T>` or have `@default_json`. Defaults apply only when a property is absent, never to an explicitly invalid value.

Rules apply recursively through nested `@json` models, arrays, lists, sets, string-keyed `Map` and `SortedMap` fields, and tuples. A tuple must arrive as a JSON array with exactly one element per position, and each element is checked against its own position's type; issue locations name the position as a decimal index. Validation collects every independent failure in declaration order instead of stopping at the first one.

## Custom checks

For a field callback, write `@validate_with(check)` and define `fun check(value: FieldType): Option<string>`. Return `None` for success or `Some(message)` for failure.

For cross-field rules, mark a static method `@model_validator`. It takes the model and returns `List<ValidationIssue>`. Custom checks run only after the built-in rules pass, so they can assume well-formed values. Errors contain locations, codes, and messages, never copies of the submitted input.

## In web handlers

A `@body` parameter whose type is an `@json` model is decoded with `Validation.decode`. A failure returns HTTP 422 with the `detail` array, and each `loc` starts with `body`. A constraint on a `@query`, `@path`, `@header`, or `@cookie` parameter also returns 422 with a location naming the parameter. See [Parameters and bodies](parameters.md).

Use `Json.deserialize` for ordinary JSON decoding without these rules. `@default_json` is validation metadata; it does not change a constructor's default arguments.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
