# ProducesAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `ProducesAttribute`

Sets the content type of a yielded string or byte stream.

```dream
public struct ProducesAttribute
```

## `content_type`

Media type sent with the streamed response, such as `text/event-stream`.

```dream
public content_type: string
```
