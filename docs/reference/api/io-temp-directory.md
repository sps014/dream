# TempDirectory

**Import:** `import system.io;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TempDirectory`

Owns a securely created private directory; destruction performs best-effort recursive cleanup.

```dream
public class TempDirectory
```

## `path`

Host path of this resource; remains available after cleanup.

```dream
public get path(): string
```

## `create`

Creates a private directory under the OS temporary directory; unsupported browser hosts return an error.

```dream
public static fun create(): Result<TempDirectory, IoError>
```

## `create_in`

Creates a private directory on the parent's filesystem for safe same-filesystem replacement.

```dream
public static fun create_in(directory: string): Result<TempDirectory, IoError>
```

## `close`

Removes the directory and its contents; repeated successful cleanup is a no-op.

```dream
public fun close(): Result<bool, IoError>
```
