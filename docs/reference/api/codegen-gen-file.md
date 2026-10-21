# GenFile

**Import:** `import system.codegen;`

## `GenFile`

A `dream.toml` `additional_files` entry.

```dream
public class GenFile
```

## `path`

Request or resource path.

```dream
public path: string
```

## `contents`

Text contents of the file.

```dream
public contents: string
```

## `constructor`

Creates a file to emit at `path` with the given `contents`.

```dream
public constructor(path: string, contents: string)
```
