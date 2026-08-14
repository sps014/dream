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
| `await File.read` / `read_bytes` | whole file |
| `await File.write_bytes` | binary |
| `await File.delete` | remove |
| `await File.create_dir` / `create_dir_all` | directories |
| `await File.list(path)` | names in a folder |
| `File.exists` / `size` / `is_dir` | sync probes |
| `File.open` / `await File.open_async` | a `FileStream` |

## `FileHandle` / `FileStream`

Open with a mode, then `read` / `write` / `seek` / `close` (sync, plus `*_async` variants except `close`). `FileStream` also has `read_all`, `has_more`, `position`, `.length`, `reset`.

## `Path`

`Path.join`, `Path.of(parts)`, `file_name`, `extension`, `parent`, `is_absolute`, `normalize`, `separator`.

Errors are [`IoError`](option-result.md). Example: [`sample/interop/file_io.dream`](https://github.com/sps014/dream/blob/main/sample/interop/file_io.dream).
