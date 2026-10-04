# MultipartForm

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class MultipartForm`

Builder for `multipart/form-data` request bodies.

```dream
public class MultipartForm
```

## `constructor`

Creates an empty form.

```dream
public constructor()
```

## `add_field`

Adds a text form field.

```dream
public fun add_field(name: string, value: string): void
```

## `add_file`

Adds a file part with raw `bytes`.

```dream
public fun add_file(name: string, filename: string, content_type: string, bytes: byte[]): void
```

## `build`

Returns `(Content-Type headers, body bytes)` for `request_bytes`.

```dream
public fun build(): MultipartBuilt
```
