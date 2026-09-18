# Files

**Import:** `import system.io;`

Whole-file helpers are `async` and return `Result`. Call them from `async fun main()`.

```dream
import system;
import system.io;

async fun main(): void {
    File.write("notes.txt", "hello\n").await;
    let text = (File.read("notes.txt").await).unwrap_or("");
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
| `File.write.await` / `append` | UTF-8 text |
| `File.read.await` / `read_bytes` / `read_lines` | whole file |
| `File.write_bytes.await` / `write_lines` | binary / lines |
| `File.copy.await` / `rename` | copy a file; same-volume move |
| `File.delete.await` | remove a file |
| `File.remove_dir.await` / `remove_dir_all` | empty dir / recursive |
| `File.create_dir.await` / `create_dir_all` | directories |
| `File.list(path).await` / `list_paths` | names, or joined paths |
| `File.exists` / `size` / `is_dir` / `is_file` / `stat` | sync probes |
| `File.open` / `File.open_async.await` | a `FileStream` |

`File.stat` returns `FileStats` (`size`, `mtime_millis` / `ctime_millis` / `atime_millis`, `mode`, `kind`) with `is_file` / `is_dir` / `is_symlink`. With `import system;`, `modified()` / `created()` yield `DateTime`.

Async `File` / `FileHandle` methods take an optional last `token: Option<CancellationToken>`; a cancelled token yields `IoError` with code `ECANCELLED`.

## `FileHandle` / `FileStream`

Open with a mode, then `read` / `write` / `seek` / `tell` / `seek_end` / `read_line` / `write_text` / `close` (sync, plus `*_async` variants except `close`). `FileStream` also has `read_all`, `has_more`, `position`, `.length`, `reset`.

## `Path`

`Path.join`, `Path.of(parts)`, `file_name`, `stem`, `extension`, `with_extension`, `with_file_name`, `parent`, `is_absolute` / `is_relative`, `has_extension`, `components`, `normalize`, `absolute`, `relative_to`, `separator`.

Errors are [`IoError`](option-result.md). Example: [`sample/interop/file_io.dream`](https://github.com/sps014/dream/blob/main/sample/interop/file_io.dream).
