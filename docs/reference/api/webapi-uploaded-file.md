# Uploaded File

**Import:** `import system.webapi;`

For behavior and native target requirements, see [Native Web APIs](../../web-server/index.md).

## `UploadedFile`

One file part of a `multipart/form-data` request, bound by `@file` or returned by `HttpIncoming.form`. The contents stay in memory up to 1 MiB and then move to a temporary file, so large uploads do not fill memory. The file is deleted when it is closed, when this object is released, or when the request finishes, whichever comes first.

```dream
public class UploadedFile: AsyncReader
```

## `filename`

Original filename reported by the client.

```dream
public filename: string
```

## `content_type`

Media type the client reported for this part; empty when none was sent.

```dream
public content_type: string
```

## `headers`

Headers sent with this part, such as `Content-Disposition`.

```dream
public headers: HttpHeaders
```

## `size`

Size of the uploaded contents in bytes.

```dream
public size: long
```

## `read`

Reads up to `size` bytes from the current position; a negative `size` reads the rest of the file. An empty result means the end of the file. Fails with `ECLOSED` after `close` or once the request has finished.

```dream
public fun read(
    size: int = -1,
    token: Option<CancellationToken> = Option.None
): Future<Result<byte[], IoError>>
```

## `seek`

Moves the read position to `offset` bytes from the start of the file.

```dream
public fun seek(
    offset: long,
    token: Option<CancellationToken> = Option.None
): Future<Result<Unit, IoError>>
```

## `close`

Deletes the stored contents now; later reads fail with `ECLOSED`. Safe to call repeatedly.

```dream
public fun close(): void
```

## `read_async`

Reads up to `max_bytes` from the current position; an empty result for a positive request is the end of the file.

```dream
public async unique fun read_async(
    max_bytes: int,
    token: Option<CancellationToken> = Option.None
): Result<byte[], IoError>
```
