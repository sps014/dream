# Read and write streams

Open a stream when you need to read or write a file in parts instead of loading it all at once.

[Back to overview](file.md)

## `FileHandle` / `FileStream`

Open with a mode (`"r"`, `"w"`, `"a"`, `"r+"`, `"w+"`, `"a+"`), then `read` / `write` / `seek` / `tell` / `seek_end` / `read_line` / `write_text` / `close` (sync, plus `*_async` variants except `close`). `FileStream` also has `read_all`, `has_more`, `position`, `.length`, `reset`.
