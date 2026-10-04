# FileStats

**Import:** `import system.io;`

Read the [usage guide](../stdlib/file.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class FileStats`

Metadata from a single `stat` of a filesystem path. Times are UTC epoch milliseconds. `mode` is Unix `st_mode` (0 on Windows and in the browser memfs). `kind` is 0 file, 1 directory, 2 symlink, 3 other. `File.stat` follows symlinks, so `kind == 2` is uncommon.

```dream
public class FileStats
```

## `size`

```dream
public get size(): long
```

## `mtime_millis`

```dream
public get mtime_millis(): long
```

## `ctime_millis`

```dream
public get ctime_millis(): long
```

## `atime_millis`

```dream
public get atime_millis(): long
```

## `mode`

```dream
public get mode(): int
```

## `kind`

```dream
public get kind(): int
```

## `is_file`

```dream
public get is_file(): bool
```

## `is_dir`

```dream
public get is_dir(): bool
```

## `is_symlink`

```dream
public get is_symlink(): bool
```
