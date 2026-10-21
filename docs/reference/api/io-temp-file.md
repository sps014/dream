# TempFile

**Import:** `import system.io;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

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
public fun persist_replace(destination: string): Result<Unit, IoError>
```

## `close`

Closes the file and removes its private directory, reporting either cleanup failure.

```dream
public fun close(): Result<Unit, IoError>
```
