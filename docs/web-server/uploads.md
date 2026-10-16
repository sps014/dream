---
title: Uploads
summary: Accept form fields and uploaded files.
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
    public static fun upload(@form title: string, @file upload: UploadedFile): string {
        return title + ": " + upload.filename + " (" + upload.bytes.length.to_string() + " bytes)";
    }
}

async fun main(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Send `multipart/form-data` with fields named `title` and `upload`, for example `curl -F title=demo -F upload=@notes.txt http://127.0.0.1:8000/upload`.

`UploadedFile` contains `filename`, `content_type`, and `bytes`. Use `Option<UploadedFile>` for an optional upload and a positional name override such as `@file("attachment")`. Scalar form values are parsed using their declared types.

The host buffers multipart input within the 16 MiB body limit. File metadata is client-supplied; choose storage paths independently of the submitted filename and validate content according to your application. JSON body and multipart extractors cannot be combined on one route; the generator reports that combination at compile time.

OpenAPI describes file fields as binary multipart properties. Swagger UI provides upload controls for those fields.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
