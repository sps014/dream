# UploadedFile

**Import:** `import system.webapi;`

Read the [usage guide](../stdlib/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class UploadedFile`

One `multipart/form-data` file part (`@file`).

```dream
public class UploadedFile
```

## `filename: string`

```dream
public filename: string
```

## `content_type: string`

```dream
public content_type: string
```

## `bytes: byte[]`

```dream
public bytes: byte[]
```

## `constructor`

```dream
public constructor(filename: string, content_type: string, bytes: byte[])
```
