# Option and Result

No import. These two unions stand in for “missing” and “failed” — Dream has no `null`.

```dream
import system;

fun main() {
    let maybe: Option<int> = Some(3);
    System.println(maybe.unwrap_or(0));

    let parsed = int.parse("42");   // Result<int, ParseError>
    switch (parsed) {
        Ok(n) => System.println(n),
        Err(e) => System.println(e.message()),
    }
}
```

## `Option<T>` — `Some(value)` or `None`

| Method | Meaning |
| --- | --- |
| `is_some()` / `is_none()` | which arm? |
| `unwrap_or(fallback)` | value, or a default |
| `unwrap()` / `expect(msg)` | value, or panic |
| `unwrap_or_else(f)` | value, or call `f` |
| `filter(pred)` / `ok_or(err)` | keep / lift into `Result` |
| `map(f)` | transform the inner value |
| `and_then(f)` | chain another `Option` |
| `or(other)` | this, or `other` if `None` |

Unpack with `switch` when both arms matter.

## `Result<T, E>` — `Ok(value)` or `Err(error)`

| Method | Meaning |
| --- | --- |
| `is_ok()` / `is_err()` | which arm? |
| `unwrap_or(fallback)` | value, or a default |
| `unwrap()` / `expect(msg)` / `unwrap_err()` | panic on the other arm |
| `ok()` | `Option` of the success value |
| `map(f)` / `map_err(f)` | transform success or error |
| `and_then(f)` | chain another `Result` |

File, HTTP, GPU, and parse APIs return `Result`. In an `async` function (or any function that itself returns `Result`), `?` forwards `Err` to the caller.

## `?` — try-propagation

`expr?` unwraps `Ok` or returns `Err` from the current function. The function’s return type must be a `Result`.

`Option` and `Result` do not mix, exactly as in Rust: `?` on an `Option` needs the enclosing function to return a matching `Option`. `?` on a `Result` rebuilds `Err` at the function’s `Result` type: `E` may widen when the operand’s error implements the function’s error interface (`GpuError` → `Error`), but there is no general `From` conversion between unrelated error types. Bridge those explicitly:

```dream
let first = first_line(text).ok_or(ParseError.invalid("empty input"))?;   // Option → Result
let n = int.parse(text).map_err(fun(e: ParseError): ConfigError => ConfigError.from_parse(e))?;
```

A `Result<T, ConcreteError>` is also assignable to `Result<T, Error>` when `ConcreteError` implements `Error` (the heap box has the same layout: discriminant plus a reference payload). Use `map_err` when the payload would change representation (a value struct boxed into an interface).

## `?` in `main`

`main` may return `Result<T, E>`, so `?` works at the top level. An `Err` that reaches `main` is printed to standard error as `Error: <e>` and the process exits 1:

```dream
import system;

fun parse_port(borrow text: string): Result<int, ParseError> {
    let port = int.parse(text)?;
    if port < 1 || port > 65535 {
        return Result.Err(ParseError.invalid("port out of range: " + port.to_string()));
    }
    return Result.Ok(port);
}

fun main(): Result<bool, ParseError> {
    let port = parse_port("8080")?;
    System.println("listening on " + port.to_string());

    let bad = parse_port("not-a-number")?;   // Err: returns from main right here
    System.println("never reached " + bad.to_string());
}   // falling off the end of main is an implicit Result.Ok(true)
```

Dream has no unit type, so the success slot is spelled `bool`. `main` alone may fall off the end when it returns `Result<bool, E>`, which the compiler treats as `return Result.Ok(true);` — an explicit one stays legal, and any other `Result<T, E>` still requires a return on every path.

`async fun main(): Result<T, E>` works the same way, and `?` binds to the awaited value, so no parentheses are needed:

```dream
async fun main(): Result<bool, IoError> {
    let text = File.read("config.json").await?;
    System.println(text);
}
```

An `Option` main is rejected — bridge it with `ok_or` as above. `main(): int` is also allowed, and returns the process exit code directly, as in C; falling off the end means `return 0`.

## Errors

Types that implement `Error` have `.message()` and `.code()`. Common ones: `ParseError` (bootstrap), `ArgError` (`import system;`), `IoError` (`system.io`).
