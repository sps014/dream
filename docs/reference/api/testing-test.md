# Test

**Import:** `import system.testing;`

Read the [usage guide](../../library/system/testing.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Test`

The runner `dream test` synthesizes calls one of these per `@test` function. Each case prints `RUN <name>` before its body and `PASS <name>` after it. An `Assert.*` failure, a panic or a trap ends the process, so the last `RUN` line names the case that was running.

```dream
public static class Test
```

## `run`

Runs a named test body and records its success or failure.

```dream
public static fun run(name: string, body: fun(): void): void
```

## `run_with_timeout`

The driver reads the limit back from the `RUN` line and stops a case that exceeds it.

```dream
public static fun run_with_timeout(name: string, timeout_ms: int, body: fun(): void): void
```

## `run_async`

Runs a named asynchronous test and records its success or failure.

```dream
public static async fun run_async(name: string, body: fun(): Future<void>): void
```

## `run_async_with_timeout`

Runs a named asynchronous test, failing if it exceeds `timeout_ms` milliseconds.

```dream
public static async fun run_async_with_timeout(
    name: string,
    timeout_ms: int,
    body: fun(): Future<void>
): void
```
