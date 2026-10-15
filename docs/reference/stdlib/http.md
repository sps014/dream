# HTTP clients and streamed responses

Import `system.net` for HTTP clients and `system.io` when copying responses to
files. Native clients use reqwest on Tokio; Node and browsers use Fetch.

```dream
import system;
import system.net;

async fun main(): Result<bool, HttpError> {
    let parsed = Url.parse("https://example.com/");
    if parsed.is_err() { return Result.Err(HttpError("EINVAL", "invalid URL")); }
    let client = HttpClient();
    let request = HttpRequest.get(parsed.unwrap());
    request.headers.set("Accept", "text/html");
    let response = client.send(request, options: HttpOptions(timeout_ms: 10000)).await?;
    System.println(response.status);
    let text = response.read_text(max_bytes: 1048576).await?;
    response.close()?;
    return client.close();
}
```

Reuse a client for multiple requests. It snapshots each request's URL, headers,
options, and byte body when sending begins. `get`, `post`, `put`, `patch`, and
`delete` create requests; the constructor also accepts `HEAD` and `OPTIONS`.
`set_text` encodes UTF-8 without choosing a content type. Set that header explicitly
when required. GET and HEAD bodies and URLs containing credentials are rejected.
HTTP 4xx/5xx statuses are successful transport responses; inspect `status` yourself.

Headers use case-insensitive names with `append`, `set`, `get`, `get_all`, and
`remove`. Native response headers preserve repeated values. Fetch can combine
repeated values and browser CORS rules can hide response headers. Node preserves
separate `Set-Cookie` values where its Fetch API exposes them.

## Streaming and cleanup

`send` completes when response headers arrive. `response.body` implements
`AsyncReader` and returns up to the requested bytes; empty successful reads at a
positive size mean EOF. Use `Streams.copy_async(response.body, destination)` for
bounded-memory downloads. The native adapter keeps one transport chunk plus the
requested output; Fetch adapters retain one received chunk between reads.

`read_text` strictly decodes incremental UTF-8, defaulting to a 1 MiB byte limit.
It consumes one extra byte when detecting a limit violation. Encoding and limit
errors do not restore already consumed input. Close the response after errors.

Close every response, including unfinished bodies. Closure aborts a pending body
read and is idempotent. A retained `body` remains owned independently of its
response; closing either closes that body for all aliases. Destructors release
clients, unfinished bodies, and abandoned operations without throwing.

Client closure cancels pending sends and releases its connection pools, while
already received responses remain independently usable. Native clients retain
at most 32 combinations of connection timeout and redirect policy. Each host
bounds live clients, bodies, and operation records to 1024 each; exhaustion
reports `EBUSY` or `ELIMIT`.

## Deadlines, redirects, and cancellation

`HttpOptions` defaults to a 30-second total deadline, no separate connection
timeout, and the host redirect policy (`max_redirects: -1`). Zero deadline means
no configured deadline. The deadline includes streamed-body consumption.
Native and Node host-default redirect policy follows at most ten redirects.
Explicit zero redirects exposes the redirect response; positive values enforce
a limit and return `EREDIRECT` when exceeded.

`CancellationToken` is accepted by `send`, `read_text`, and body reads. Cancellation
returns `ECANCELLED`, abandons pending transport work, and does not roll back
request side effects or consumed input. Overlapping body reads report `EBUSY`.
HTTP completion and cancellation wake the scheduler through notifications. No
periodic timer checks are needed while a request or body read is pending.

`HttpError.code()` distinguishes validation (`EINVAL`), deadline (`ETIMEOUT`),
redirect (`EREDIRECT`), transport/TLS (`ETRANSPORT`), cancellation (`ECANCELLED`),
closure (`ECLOSED`), unsupported controls (`ENOTSUP`), limits (`ELIMIT`), and UTF-8
(`EENCODING`). Body reads expose corresponding `IoError` codes to compose with
other streams. Transport errors omit request URLs from their messages.

## Browser capabilities

Use `HttpCapabilities.connection_timeouts`, `.redirect_limits`, and
`.unrestricted_headers` to inspect these controls before sending.

Fetch hosts reject nonzero `connect_timeout_ms`, since Fetch exposes no separate
connection timeout. Browser hosts also require `max_redirects: -1`; the browser
controls redirect following and does not reliably expose redirect responses or
counts. Native and Node support explicit redirect limits.

Browsers reject restricted request headers with `ENOTSUP`, including connection,
cookie, origin, host, content-length, and `Sec-`/`Proxy-` headers. CORS, mixed-content,
and browser certificate rules still apply. Opaque responses return `ENOTSUP`.
Fetch requests omit ambient credentials. No API disables certificate validation.
Node's Fetch connection pool is managed by the Node host rather than by an
individual Dream client, so closing a Dream client cannot explicitly drain it.

## Cancellation before sending

A token cancelled before `send` returns `ECANCELLED` without issuing a request.
This example runs without network access:

```dream
import system;
import system.net;

async fun main(): Result<bool, HttpError> {
    let client = HttpClient();
    let cancellation = CancellationSource();
    cancellation.cancel();
    let request = HttpRequest.get(Url.parse("https://example.com/").unwrap());
    request.headers.append("X-Role", "reader");
    request.headers.append("x-role", "editor");
    System.println(request.headers.get_all("X-ROLE").length);
    let result = client.send(request, token: Option.Some(cancellation.token)).await;
    System.println(result.unwrap_err().code());
    return client.close();
}
```
