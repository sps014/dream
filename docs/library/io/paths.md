---
title: Paths
summary: "Build, split, and normalize file paths."
icon: material/folder-marker-outline
api: reference/api/system-io.md
---

# Work with file paths

Build and inspect paths without manually joining separators. Path helpers operate on
text; they do not check whether a file exists or resolve symbolic links.

**Import:** `import system.io;`

```dream
import system;
import system.io;

fun main(): void {
    let path = Path.join("docs", "notes.txt");
    System.println(Path.file_name(path).unwrap_or(""));
    System.println(Path.extension(path).unwrap_or(""));
    System.println(Path.stem(path));
}
```

Save this as `paths.dream` and run `dream run paths.dream`. It prints:

```text
notes.txt
txt
notes
```

All helpers are static members of `Path`. Both `/` and `\` are recognized as
separators when splitting or inspecting a path.

## Build and change paths

| Function | Returns | Effect |
|---|---|---|
| `join(a, b, ...more)` | `string` | Joins segments, skipping empty ones and avoiding doubled separators. |
| `of(parts)` | `string` | Joins an array of segments with the same rules. |
| `with_extension(path, ext)` | `string` | Replaces the last extension, or adds one; `ext` may start with `.`. |
| `with_file_name(path, name)` | `string` | Replaces the final component. |
| `separator` | `string` | Separator used when joining; always `/`. |

Pass extra segments to `join` as trailing arguments, for example
`Path.join("a", "b", "c")`. Native hosts accept `/` on Windows too.

## Inspect a path

| Function | Returns | Effect |
|---|---|---|
| `file_name(path)` | `Option<string>` | Final component; `None` for empty input or a trailing separator. |
| `extension(path)` | `Option<string>` | Text after the last `.` in the file name. |
| `parent(path)` | `Option<string>` | Everything before the last separator. |
| `stem(path)` | `string` | File name without its last extension. |
| `is_absolute(path)` | `bool` | Starts with `/`, `\`, or a drive such as `C:`. |
| `is_relative(path)` | `bool` | Opposite of `is_absolute`. |
| `has_extension(path, ext)` | `bool` | Last extension equals `ext`, with or without a leading `.`. |
| `components(path)` | `string[]` | Nonempty components split on `/` and `\`. |

A path without the requested component returns `None`, so handle that case with
a switch or a default. `stem` returns an empty string when there is no filename,
and keeps earlier extensions: the stem of `a.tar.gz` is `a.tar`.

## Normalize and resolve

| Function | Returns | Effect |
|---|---|---|
| `normalize(path)` | `string` | Removes `.` and resolves `..` where possible. |
| `absolute(path)` | `string` | Joins a relative path onto the working directory, then normalizes. |
| `relative_to(path, base)` | `Option<string>` | Suffix of `path` under `base`, or `None`. |

None of these consult the filesystem or resolve symbolic links. `normalize`
returns `.` when a relative path simplifies to nothing, and drops `..` that would
climb above the root of an absolute path. `relative_to` compares normalized text
and returns `.` when both paths are equal.

Use the [file API](file.md) when you need existence checks, file contents, or filesystem
errors. The [Path reference](../../reference/api/io-path.md) lists every signature.
