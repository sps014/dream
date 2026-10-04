# File

**Import:** `import system.io;`

Read the [usage guide](../stdlib/file.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class File`

`File` - a small cross-runtime filesystem API.

```dream
public static class File
```

## `read`

Reads the whole file at `path` as UTF-8 text.

```dream
public static async fun read(borrow path: string, token: Option<CancellationToken> = Option.None): Result<string, IoError>
```

## `write`

Overwrites `path` with `content`; resolves with `Ok(bytes_written)` or `Err` on failure.

```dream
public static async fun write(borrow path: string, borrow content: string, token: Option<CancellationToken> = Option.None): Result<long, IoError>
```

## `append`

Appends `content` to the end of `path`; resolves with `Ok(bytes_written)` or `Err`.

```dream
public static async fun append(borrow path: string, borrow content: string, token: Option<CancellationToken> = Option.None): Result<long, IoError>
```

## `read_bytes`

Reads the whole file at `path` as raw bytes.

```dream
public static async fun read_bytes(borrow path: string, token: Option<CancellationToken> = Option.None): Result<byte[], IoError>
```

## `write_bytes`

Writes raw bytes to `path`, replacing any existing contents.

```dream
public static async fun write_bytes(borrow path: string, borrow data: byte[], token: Option<CancellationToken> = Option.None): Result<long, IoError>
```

## `delete`

Deletes `path`; resolves `Ok(true)` on success.

```dream
public static async fun delete(borrow path: string, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

## `copy`

Copies a file from `from` to `to`, overwriting `to` when it already exists.

```dream
public static async fun copy(borrow from: string, borrow to: string, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

## `rename`

Renames or moves `from` to `to` (same-volume when the OS requires it).

```dream
public static async fun rename(borrow from: string, borrow to: string, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

## `remove_dir`

Removes an empty directory at `path`.

```dream
public static async fun remove_dir(borrow path: string, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

## `remove_dir_all`

Recursively deletes `path` (file or directory tree).

```dream
public static async fun remove_dir_all(borrow path: string, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

## `create_dir`

Creates a single directory at `path`; resolves `Ok(true)` on success.

```dream
public static async fun create_dir(borrow path: string, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

## `create_dir_all`

Creates `path` and any missing parents; resolves `Ok(true)` on success.

```dream
public static async fun create_dir_all(borrow path: string, token: Option<CancellationToken> = Option.None): Result<bool, IoError>
```

## `list`

Lists the entries of directory `path`, or `Err(IoError)` when the path is missing or not a directory (an existing empty directory yields `Ok([])`).

```dream
public static async fun list(borrow path: string, token: Option<CancellationToken> = Option.None): Result<string[], IoError>
```

## `list_paths`

Like `list`, but each name is joined onto `path`. Fails with the same `IoError` cases.

```dream
public static async fun list_paths(borrow path: string, token: Option<CancellationToken> = Option.None): Result<string[], IoError>
```

## `read_lines`

Reads the whole file and splits on `\n`, dropping a trailing empty part from a final newline.

```dream
public static async fun read_lines(borrow path: string, token: Option<CancellationToken> = Option.None): Result<string[], IoError>
```

## `write_lines`

Writes `lines` joined by newlines, including a trailing newline when `lines` is non-empty.

```dream
public static async fun write_lines(borrow path: string, borrow lines: string[], token: Option<CancellationToken> = Option.None): Result<long, IoError>
```

## `exists`

Cheap synchronous predicates (no async overhead).

```dream
public static fun exists(borrow path: string): bool
```

## `size`

Size of `path` in bytes, or `None` if it does not exist.

```dream
public static fun size(borrow path: string): Option<long>
```

## `is_dir`

```dream
public static fun is_dir(borrow path: string): bool
```

## `is_file`

True when `path` exists and is a regular file.

```dream
public static fun is_file(borrow path: string): bool
```

## `stat`

One `stat` of `path`. Follows symlinks. Missing paths are `Err`.

```dream
public static fun stat(borrow path: string): Result<FileStats, IoError>
```

## `open`

Opens a seekable read stream over `path` (OS-backed; no whole-file preload).

```dream
public static fun open(borrow path: string): Result<FileStream, IoError>
```

## `open_async`

Async wrapper around `open`.

```dream
public static async fun open_async(borrow path: string, token: Option<CancellationToken> = Option.None): Result<FileStream, IoError>
```
