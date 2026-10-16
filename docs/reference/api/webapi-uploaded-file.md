# Uploaded File

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../../web-apis/index.md).

## `UploadedFile`

One `multipart/form-data` file part (`@file`).

```dream
public class UploadedFile
```

## `filename`

Original filename reported with the upload.

```dream
public filename: string
```

## `content_type`

Media type of the response body.

```dream
public content_type: string
```

## `bytes`

Contents as a byte array.

```dream
public bytes: byte[]
```

## `constructor`

Creates a `UploadedFile` with `filename`, `content_type`, `bytes`.

```dream
public constructor(filename: string, content_type: string, bytes: byte[])
```
