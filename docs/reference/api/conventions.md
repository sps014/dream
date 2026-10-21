---
title: Error conventions
summary: How standard-library APIs report missing values, failures, and programming errors.
icon: material/alert-circle-outline
---

# Error conventions

Every standard-library API reports a problem in one of three ways. The kind of problem decides which one: a value that might be missing, an operation that can fail at run time, or a call that breaks the API's rules.

## Missing values return `Option`

A lookup that may find nothing returns `Option<T>`. Examples are `Map.get(key)`, `JsonValue.get(key)`, `List.first()`, and `string.index_of(text)`. Handle `None` when absence is expected, or supply a default with `unwrap_or`:

```dream
import system;
import system.collections;

fun main(): void {
    let ages = Map<string, int>();
    ages["Ada"] = 36;

    let known = ages.get("Ada").unwrap_or(0);
    let unknown = ages.get("Grace").unwrap_or(0);

    System.println(known); // 36
    System.println(unknown); // 0
}
```

## Indexers panic on a missing key or index

`value[key]` and `value[index]` assume the entry exists. `List`, arrays, `Map`, `SortedMap`, and `JsonValue` all panic on a missing key or an out-of-range index. Call `contains(key)` or the `Option`-returning `get(key)` first when the entry might not exist.

`slice(start, end)` follows the same rule on lists, arrays, and spans: it panics unless `0 <= start <= end <= length`.

## Fallible operations return `Result`

An operation that can fail for reasons outside the program returns `Result<T, E>`. Examples are reading a file, parsing text, or sending on a closed channel. `E` always implements the `Error` interface:

| Member | Returns |
| --- | --- |
| `code()` | A stable machine code such as `ENOENT`, `EPARSE`, or `ECLOSED`, for logs and branching |
| `message()` | A readable explanation for people |

Enum errors whose variants you match on, such as `GpuError`, `ChannelError`, and `TaskPoolError`, provide the same `code()` and `message()` methods. They cannot be converted to `Error` with `?`, so return them as they are or wrap them in your own error type.

An operation with nothing to return on success returns `Result<Unit, E>` and succeeds with `Result.Ok(Unit())`. Pass the failure to your caller with `?`, or handle both cases with `switch`:

```dream
import system;
import system.io;

async fun save(path: string, text: string): Result<Unit, IoError> {
    File.write(path, text).await?;
    return Result.Ok(Unit());
}

async fun main(): void {
    switch (save("notes.txt", "hello").await) {
        Ok(_) => System.println("saved"),
        Err(e) => System.println(e.code() + ": " + e.message()),
    }
}
```

Parsing functions, such as `int.parse`, `Uuid.parse`, and `Url.parse`, fail with `ParseError` (code `EPARSE`). An invalid argument passed to a system call, such as an empty environment variable name, fails with `ArgumentError` (code `EINVAL`).

`main` may return `Result<Unit, E>`. Reaching the end of its body succeeds. An `Err` prints the error and exits with status 1.

## Programming errors panic

A call that breaks an API's documented rules panics instead of returning an error. Examples include unwrapping `None`, indexing past the end, and slicing with reversed bounds. These are bugs to fix in the calling code, not conditions to handle at run time. The API documentation states each such rule.
