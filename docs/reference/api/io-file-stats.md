# FileStats

**Import:** `import system.io;`

Read the [usage guide](../../library/io/file.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `FileStats`

File size, timestamps, mode, and entry kind returned by `File.stat`. Timestamps use UTC epoch milliseconds. `File.stat` follows symbolic links.

```dream
public class FileStats
```

## `size`

File size in bytes.

```dream
public get size(): long
```

## `mtime_millis`

Last modification time in UTC epoch milliseconds.

```dream
public get mtime_millis(): long
```

## `ctime_millis`

Last metadata-change time in UTC epoch milliseconds.

```dream
public get ctime_millis(): long
```

## `atime_millis`

Last access time in UTC epoch milliseconds.

```dream
public get atime_millis(): long
```

## `mode`

Unix file mode, or zero on platforms without Unix modes.

```dream
public get mode(): int
```

## `kind`

The kind of filesystem entry.

```dream
public get kind(): FileKind
```

## `is_file`

Returns whether the entry is a regular file.

```dream
public get is_file(): bool
```

## `is_dir`

Returns whether the path refers to a directory.

```dream
public get is_dir(): bool
```

## `is_symlink`

Returns whether the entry is a symbolic link.

```dream
public get is_symlink(): bool
```
