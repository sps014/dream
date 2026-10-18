# FileMode

**Import:** `import system.io;`

## `FileMode`

Access, creation, and truncation behavior when opening a file.

```dream
public enum FileMode
```

## `Read`

Read an existing file without changing it.

```dream
Read
```

## `Write`

Create or truncate a file for writing.

```dream
Write
```

## `Append`

Create a file if needed and append writes to its end.

```dream
Append
```

## `ReadWrite`

Read and write an existing file without truncating it.

```dream
ReadWrite
```

## `WriteRead`

Create or truncate a file for reading and writing.

```dream
WriteRead
```

## `AppendRead`

Read a file and append writes, creating it if needed.

```dream
AppendRead
```
