# Uploaded File

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../stdlib/webapi.md).

## `public class UploadedFile`

One `multipart/form-data` file part (`@file`).

```dream
public class UploadedFile
```

## `public filename: string`

```dream
public filename: string
```

## `public content_type: string`

```dream
public content_type: string
```

## `public bytes: byte[]`

```dream
public bytes: byte[]
```

## `public constructor`

```dream
public constructor(filename: string, content_type: string, bytes: byte[])
```
