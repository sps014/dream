---
title: Uploads
summary: Accept form fields and uploaded files, including uploads too large for memory.
icon: material/upload-outline
---

# Forms and uploaded files

Multipart fields use `@form`; file parts use `@file`.

```dream
import system;
import system.webapi;

@router("")
class Routes {
    @post("/upload")
    public static async fun upload(@form title: string, @file upload: UploadedFile): string {
        let contents = upload.read().await;
        if contents.is_err() {
            return "could not read " + upload.filename;
        }
        return title + ": " + upload.filename + " (" + upload.size.to_string() + " bytes)";
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Send `multipart/form-data` with fields named `title` and `upload`, for example `curl -F title=demo -F upload=@notes.txt http://127.0.0.1:8000/upload`.

| Attribute | Binds | Parameter type | Missing part |
| --- | --- | --- | --- |
| `@form` | A text field | `string`, `bool`, a number, or `Option<T>` of one | 422 `missing`, or `None` for `Option<T>` |
| `@file` | A file part | `UploadedFile` or `Option<UploadedFile>` | 422 `missing`, or `None` for `Option<UploadedFile>` |

Both match the part by parameter name; pass a positional name to override it, such as `@file("attachment")`. Form values are parsed using their declared types, and the [constraint attributes](validation.md#constraint-attributes) apply to them. JSON body and multipart extractors cannot be combined on one route; the generator reports that combination at compile time.

## Uploaded files

An `UploadedFile` keeps up to 1 MiB of its contents in memory and moves larger contents to a temporary file, so a large upload does not fill memory. Read it in one call, in chunks, or by copying it to another stream.

| `UploadedFile` member | Type | Meaning |
| --- | --- | --- |
| `filename` | `string` | The filename the client sent. |
| `content_type` | `string` | The media type the client sent for the part. |
| `headers` | `HttpHeaders` | Every header sent with the part. |
| `size` | `long` | Size of the contents in bytes. |
| `read(size = -1)` | `Result<byte[], IoError>` (async) | Reads up to `size` bytes from the current position; `-1` reads the rest. An empty result is the end. |
| `seek(offset)` | `Result<Unit, IoError>` (async) | Moves the read position to `offset` bytes from the start. |
| `close()` | `void` | Deletes the contents now. |
| `read_async(max_bytes)` | `Result<byte[], IoError>` (async) | The `AsyncReader` form of `read`, so `Streams.copy_async` accepts an `UploadedFile`. |

The temporary file is deleted when you call `close`, when the `UploadedFile` is released, or when the request finishes, whichever comes first. Reading after that fails with `ECLOSED`. To keep an upload, copy it somewhere permanent inside the handler.

File metadata comes from the client. Choose storage paths independently of the submitted filename, and validate the contents for your application.

## Form limits

`@form` and `@file` parse the body with Starlette's default limits. A request over a limit gets `400 Bad Request` with a message naming the limit.

| Limit | Default | Applies to |
| --- | --- | --- |
| `max_files` | `1000` | Number of file parts. |
| `max_fields` | `1000` | Number of text fields. |
| `max_part_size` | `1048576` | Size of each text field, in bytes. File parts have no size limit. |

To use other limits, take the `HttpIncoming` and call `form` yourself. It returns a `FormData` with every field and file:

```dream
import system;
import system.webapi;

@router("")
class Routes {
    @post("/gallery")
    public static async fun gallery(incoming: HttpIncoming): HttpOutgoing {
        let parsed = incoming.form(max_files: 20, max_part_size: 4096).await;
        if parsed.is_err() {
            return HttpOutgoing.detail(parsed.unwrap_err().message(), 400);
        }
        let form = parsed.unwrap();
        let album = form.get("album").unwrap_or("untitled");
        let photos = form.files("photo");
        return HttpOutgoing.text(album + ": " + photos.length.to_string() + " photos");
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

| `FormData` method | Returns |
| --- | --- |
| `get(name)` | The first text field named `name`, or `None`. |
| `get_all(name)` | Every text field named `name`, in the order sent. |
| `file(name)` | The first file part named `name`, or `None`. |
| `files(name)` | Every file part named `name`, in the order sent. |
| `close()` | Closes every uploaded file now. |

The parsed form is cached on the request: a later `form` call, including the one an `@form` or `@file` parameter makes, returns the same `FormData` whatever limits it passes.

## Uploads larger than a form

The server sets no limit on body size. To accept a body of any size without holding it in memory, skip multipart parsing and copy `incoming.stream()` straight to a file:

```dream
import system;
import system.io;
import system.webapi;

@router("")
class Routes {
    @put("/files/backup.bin")
    public static async fun store(incoming: HttpIncoming): HttpOutgoing {
        let opened = FileHandle.open_async("backup.bin", FileMode.Write).await;
        if opened.is_err() {
            return HttpOutgoing.detail("cannot open destination", 500);
        }
        let file = opened.unwrap();
        let copied = Streams.copy_async(incoming.stream(), file).await;
        let closed = file.close();
        if copied.is_err() || closed.is_err() {
            return HttpOutgoing.detail("upload failed", 400);
        }
        return HttpOutgoing.text("stored " + copied.unwrap().to_string() + " bytes");
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Try it with `curl -T backup.bin http://127.0.0.1:8000/files/backup.bin`. See [Reading a request body as a stream](streaming.md#reading-a-request-body-as-a-stream) for the rules on mixing `stream()` with the other body readers.

OpenAPI describes file fields as binary multipart properties. Swagger UI provides upload controls for those fields.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
