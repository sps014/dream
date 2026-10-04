# Work with file paths

Build and inspect paths without manually joining separators. Path helpers work on path text; check a file separately when you need to know whether it exists.

[Back to overview](file.md)

## `Path`

`Path.join`, `Path.of(parts)`, `file_name`, `stem`, `extension`, `with_extension`, `with_file_name`, `parent`, `is_absolute` / `is_relative`, `has_extension`, `components`, `normalize`, `absolute`, `relative_to`, `separator`.

```dream
let p = Path.join("docs", "notes.txt");
System.println(Path.file_name(p).unwrap_or(""));
System.println(Path.extension(p).unwrap_or(""));
```

Errors are [`IoError`](option-result.md). Example: [`sample/interop/file_io.dream`](https://github.com/sps014/dream/blob/main/sample/interop/file_io.dream).
