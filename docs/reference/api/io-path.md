# Path

**Import:** `import system.io;`

Read the [usage guide](../stdlib/file.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Path`

Path helpers for joining and inspecting filesystem paths.

```dream
public static class Path
```

## `join`

Joins path segments left-to-right with the platform separator, avoiding double separators and skipping empty segments. Two-argument calls compile exactly as before; three or more trailing arguments pack into `more` automatically (no spread operator at the call site).

```dream
public static fun join(a: string, b: string, ...more: string[]): string
```

## `of`

Joins every segment in `parts` left-to-right with the same rules as `join`.

```dream
public static fun of(parts: string[]): string
```

## `file_name`

Final path component after the last `/` or `\`, or `None` for a trailing separator.

```dream
public static fun file_name(path: string): Option<string>
```

## `extension`

Substring after the last `.` in the file name, or `None` when missing.

```dream
public static fun extension(path: string): Option<string>
```

## `parent`

Directory portion of `path`, or `None` when there is no separator.

```dream
public static fun parent(path: string): Option<string>
```

## `is_absolute`

True for POSIX absolute paths (`/`…), Windows drive paths (`C:`…), or `\`-rooted paths.

```dream
public static fun is_absolute(path: string): bool
```

## `normalize`

Resolves `.` and `..` segments where possible. Does not consult the filesystem.

```dream
public static fun normalize(path: string): string
```

## `separator`

Host path separator. Always `/` in the WASM stdlib surface (native hosts accept `/` even on Windows); use `OsFamily` at call sites when Windows-style paths are required.

```dream
public static get separator(): string
```

## `stem`

File name without the last extension (`"a.tar.gz"` → `"a.tar"`).

```dream
public static fun stem(path: string): string
```

## `with_extension`

Replaces the last extension, or appends `ext` when the path has none. `ext` may include a leading `.`.

```dream
public static fun with_extension(path: string, ext: string): string
```

## `with_file_name`

Replaces the final path component with `name`.

```dream
public static fun with_file_name(path: string, name: string): string
```

## `is_relative`

Inverse of `is_absolute`.

```dream
public static fun is_relative(path: string): bool
```

## `has_extension`

True when the last extension equals `ext` (with or without a leading `.`).

```dream
public static fun has_extension(path: string, ext: string): bool
```

## `components`

Splits `path` on `/` and `\`, skipping empty segments.

```dream
public static fun components(path: string): string[]
```

## `absolute`

Joins `path` onto the process cwd when it is relative, then `normalize`s. Does not consult `realpath`.

```dream
public static fun absolute(path: string): string
```

## `relative_to`

String-prefix relative path after normalize (not `realpath`). `None` when `path` is not under `base`.

```dream
public static fun relative_to(path: string, base: string): Option<string>
```
