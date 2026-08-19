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

## Errors

Types that implement `Error` have `.message()` and `.code()`. Common ones: `ParseError` (bootstrap), `ArgError` (`import system;`), `IoError` (`system.io`).
