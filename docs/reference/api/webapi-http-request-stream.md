# Http Request Stream

**Import:** `import system.webapi;`

For behavior and examples, see [Streaming](../../web-server/streaming.md).

## `HttpRequestStream`

The body of one inbound request, read in chunks as it arrives from the client. When the body was already read with `read_body_bytes` or `read_body_text`, the stream replays those cached bytes. Once streaming starts, the whole-body readers and `HttpIncoming.form` fail with `ECLOSED`, because the bytes are no longer buffered.

```dream
public class HttpRequestStream: AsyncReader
```

## `read_async`

Reads up to `max_bytes` of the body; an empty result for a positive request is the end of the body. Zero consumes nothing. The request's cancellation applies when `token` is omitted.

```dream
public async unique fun read_async(
    max_bytes: int,
    token: Option<CancellationToken> = Option.None
): Result<byte[], IoError>
```
