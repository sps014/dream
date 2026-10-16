# Fixture

**Import:** `import system.testing;`

## `TestFixture`

Owns one fixture and runs its cleanup on explicit close, scope exit, or async-frame cancellation.

```dream
public class TestFixture<T, E>
```

## `constructor`

Registers deterministic cleanup before the fixture is exposed to test code.

```dream
public constructor(value: T, cleanup: fun(T): Result<bool, E>)
```

## `resource`

Borrows the fixture value while this owner remains alive.

```dream
public get resource(): T
```

## `close`

Executes cleanup once and returns the same outcome to repeated callers.

```dream
public fun close(): Result<bool, E>
```

## `Fixtures`

Helpers for async test bodies that need deterministic temporary-resource ownership.

```dream
public static class Fixtures
```

## `temporary_file`

Creates an owned temporary file whose destructor cleans up after a failed or cancelled body.

```dream
public static fun temporary_file(): Result<TempFile, IoError>
```

## `temporary_directory`

Creates an owned private directory for a test's temporary resources.

```dream
public static fun temporary_directory(): Result<TempDirectory, IoError>
```

## `run`

Runs an async body and cleanup, preserving a body error and otherwise reporting cleanup errors.

```dream
public static async fun run<T, E>(
    fixture: TestFixture<T, E>,
    body: fun(T): Future<Result<bool, E>>
): Result<bool, E>
```
