# Test

**Import:** `import system.testing;`

Read the [usage guide](../stdlib/testing.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Test`

Test runner helpers emit `RUN` and `PASS` markers for `dream test`. Assertion failures, panics, and traps end the process; timeout markers let the driver stop an overdue test.

```dream
public static class Test
```

## `run`

Prints a `RUN` marker before invoking the test and a `PASS` marker after success. An assertion failure, panic, or trap ends the process; the driver attributes it to the last `RUN` marker.

```dream
public static fun run(name: string, body: fun(): void): void
```

## `run_with_timeout`

The driver reads the limit back from the `RUN` line and stops a case that exceeds it.

```dream
public static fun run_with_timeout(name: string, timeout_ms: int, body: fun(): void): void
```

## `run_async`

Prints `RUN <name>`, awaits the test body, and prints `PASS <name>` after successful completion.

```dream
public static async fun run_async(name: string, body: fun(): Future<void>): void
```

## `run_async_with_timeout`

Runs an async test and includes `timeout_ms` in the `RUN` marker for the driver to enforce.

```dream
public static async fun run_async_with_timeout(name: string, timeout_ms: int, body: fun(): Future<void>): void
```
