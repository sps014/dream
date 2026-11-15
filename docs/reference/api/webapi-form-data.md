# Form Data

**Import:** `import system.webapi;`

For behavior and examples, see [File uploads](../../web-server/uploads.md).

## `FormData`

The parsed fields of a `multipart/form-data` request, returned by `HttpIncoming.form`. Text fields and file parts keep the order the client sent them in; a name can repeat.

```dream
public class FormData
```

## `get`

Returns the first text field named `name`.

```dream
public fun get(name: string): Option<string>
```

## `get_all`

Returns every text field named `name`, in the order they were sent.

```dream
public fun get_all(name: string): List<string>
```

## `file`

Returns the first file part named `name`.

```dream
public fun file(name: string): Option<UploadedFile>
```

## `files`

Returns every file part named `name`, in the order they were sent.

```dream
public fun files(name: string): List<UploadedFile>
```

## `close`

Closes every uploaded file now instead of when the request finishes.

```dream
public fun close(): void
```
