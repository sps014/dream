# Temporary

**Import:** `import system.io;`

## `TempCapabilities`

Availability of secure OS temporary resources on this target.

```dream
public static class TempCapabilities
```

## `supported`

True on native and Node; browser virtual filesystems do not provide OS-private resources.

```dream
public static get supported(): bool
```

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

## `TempFile`

Owns a read/write file inside a private temporary directory and cleans both deterministically.

```dream
public class TempFile
```

## `file`

Borrowed access to the owned temporary file.

```dream
public get file(): FileHandle
```

## `path`

Path of the temporary file before persistence; its containing directory is private.

```dream
public get path(): string
```

## `create`

Creates a temporary read/write file under the OS temporary directory.

```dream
public static fun create(): Result<TempFile, IoError>
```

## `create_in`

Creates a private file on the selected directory's filesystem.

```dream
public static fun create_in(directory: string): Result<TempFile, IoError>
```

## `persist_replace`

Flushes and closes the file, then atomically replaces a destination on the same filesystem. Failed replacement leaves the destination and temporary file intact; durability is not implied.

```dream
public fun persist_replace(destination: string): Result<bool, IoError>
```

## `close`

Closes the file and removes its private directory, reporting either cleanup failure.

```dream
public fun close(): Result<bool, IoError>
```
