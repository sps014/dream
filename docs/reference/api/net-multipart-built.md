# MultipartBuilt

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct MultipartBuilt`

Result of `MultipartForm.build`.

```dream
public struct MultipartBuilt
```

## `headers: HttpHeaders`

Headers including `Content-Type` with boundary.

```dream
public headers: HttpHeaders
```

## `body: byte[]`

Encoded multipart body.

```dream
public body: byte[]
```
