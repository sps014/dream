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

## Build and change paths

`Path.join(a, b, ...more)` joins segments with the platform separator, skipping empty
segments. `Path.of(parts)` accepts an array of segments. `with_extension` replaces a
filename's extension; `with_file_name` replaces the final component.

## Inspect a path

`file_name`, `extension`, and `parent` return `Option<string>`. A path without
the requested component returns `None`, so handle that case with a switch or a default.
`stem` returns the filename without its last extension, or an empty string when there is no filename.
`is_absolute`, `is_relative`, and `has_extension` return booleans.
`components` returns the nonempty components as an array.

## Normalize and resolve

`normalize` simplifies `.` and `..` components without consulting the filesystem.
`absolute` combines a relative path with the process working directory and normalizes it.
`relative_to(path, base)` returns a relative suffix when the normalized path is under
`base`, otherwise `None`. It does not resolve symlinks.

Use the [file API](file.md) when you need existence checks, file contents, or filesystem
errors. The [Path reference](../api/io-path.md) lists every signature.
