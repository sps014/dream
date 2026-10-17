# Fixtures

**Import:** `import system.testing;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
