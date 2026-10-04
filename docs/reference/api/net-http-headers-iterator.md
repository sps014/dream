# HttpHeadersIterator

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class HttpHeadersIterator : Iterator<KeyValuePair<string, string>>`

Cursor produced by `HttpHeaders.iterator()`.

```dream
public class HttpHeadersIterator : Iterator<KeyValuePair<string, string>>
```

## `next`

```dream
public fun next(): Option<KeyValuePair<string, string>>
```
