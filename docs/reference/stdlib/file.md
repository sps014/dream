# Files

**Import:** `import system.io;`

Whole-file helpers are `async` and return `Result`. Call them from `async fun main()`.

```dream
import system;
import system.io;

async fun main(): void {
    await File.write("notes.txt", "hello\n");
    let text = (await File.read("notes.txt")).unwrap_or("");
    System.println(text);
}
```

| Runtime | Where files live |
| --- | --- |
| Native / Node | Real disk |
| Browser | In-memory, gone on reload |

## Whole file (`File`)

| Call | Meaning |
| --- | --- |
| `await File.write` / `append` | UTF-8 text |
| `await File.read` / `read_bytes` / `read_lines` | whole file |
| `await File.write_bytes` / `write_lines` | binary / lines |
| `await File.copy` / `rename` | copy a file; same-volume move |
| `await File.delete` | remove a file |
| `await File.remove_dir` / `remove_dir_all` | empty dir / recursive |
| `await File.create_dir` / `create_dir_all` | directories |
| `await File.list(path)` / `list_paths` | names, or joined paths |
| `File.exists` / `size` / `is_dir` / `is_file` / `stat` | sync probes |
| `File.open` / `await File.open_async` | a `FileStream` |

`File.stat` returns `FileStats` (`size`, `mtime_millis` / `ctime_millis` / `atime_millis`, `mode`, `kind`) with `is_file` / `is_dir` / `is_symlink`. With `import system;`, `modified()` / `created()` yield `DateTime`.

## `FileHandle` / `FileStream`

Open with a mode, then `read` / `write` / `seek` / `tell` / `seek_end` / `read_line` / `write_text` / `close` (sync, plus `*_async` variants except `close`). `FileStream` also has `read_all`, `has_more`, `position`, `.length`, `reset`.

## `Path`

`Path.join`, `Path.of(parts)`, `file_name`, `stem`, `extension`, `with_extension`, `with_file_name`, `parent`, `is_absolute` / `is_relative`, `has_extension`, `components`, `normalize`, `absolute`, `relative_to`, `separator`.

Errors are [`IoError`](option-result.md). Example: [`sample/interop/file_io.dream`](https://github.com/sps014/dream/blob/main/sample/interop/file_io.dream).
