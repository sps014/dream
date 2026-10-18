---
title: HTTP
summary: Send HTTP requests and stream responses.
icon: material/web
api: reference/api/system-net.md
---

# HTTP clients and streamed responses

Import `system.net` for HTTP clients and `system.io` when copying responses to
files. Native clients use reqwest on Tokio; Node and browsers use Fetch.

```dream
import system;
import system.net;

async fun main(): Result<bool, HttpError> {
    let parsed = Url.parse("https://example.com/");
    if parsed.is_err() {
        return Result.Err(HttpError("EINVAL", "invalid URL"));
    }
    let client = HttpClient();
    let request = HttpRequest(HttpMethod.Get, parsed.unwrap());
    request.headers.set("Accept", "text/html");
    let response = client.send(request, options: HttpOptions(timeout_ms: 10000)).await?;
    System.println(response.status);
    let text = response.read_text(max_bytes: 1048576).await?;
    response.close()?;
    return client.close();
}
```

Reuse a client for multiple requests. It snapshots each request's URL, headers,
options, and byte body when sending begins. HTTP 4xx/5xx statuses are successful
transport responses; inspect `status` yourself.

## Client

| Member | Returns | Effect |
|---|---|---|
| `HttpClient()` | `HttpClient` | Creates a client; allocation failures surface from `send`. |
| `send(request, token)` | `async Result<HttpResponse, HttpError>` | Sends with default `HttpOptions()`. |
| `send(request, options, token)` | `async Result<HttpResponse, HttpError>` | Sends with explicit options; completes when headers arrive. |
| `close()` | `Result<bool, HttpError>` | Cancels pending sends and releases pools; idempotent. |

`token` is an optional `Option<CancellationToken>` and defaults to `None`.

## Requests

| Member | Kind | Effect |
|---|---|---|
| `HttpRequest(method, url)` | constructor | Creates a request with empty headers and body. |
| `HttpRequest(HttpMethod.Get, url)` | static | Creates a `GET` request. |
| `HttpRequest(HttpMethod.Post, url)` | static | Creates a `POST` request. |
| `HttpRequest(HttpMethod.Put, url)` | static | Creates a `PUT` request. |
| `HttpRequest(HttpMethod.Patch, url)` | static | Creates a `PATCH` request. |
| `HttpRequest(HttpMethod.Delete, url)` | static | Creates a `DELETE` request. |
| `method` | `string` field | `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, or `OPTIONS`. |
| `url` | `Url` field | Absolute `http`/`https` destination. |
| `headers` | `HttpHeaders` field | Request headers. |
| `body` | `byte[]` field | Body bytes, copied on each send. |
| `set_text(text)` | method | Replaces the body with UTF-8 bytes. |

`set_text` does not choose a content type; set that header explicitly when
required. GET and HEAD bodies and URLs containing credentials are rejected.

## Headers

Header names are case-insensitive and repeated values keep their order.

| Member | Returns | Effect |
|---|---|---|
| `HttpHeaders()` | `HttpHeaders` | Creates an empty collection. |
| `length` | `int` | Number of stored pairs, counting repeated names. |
| `append(name, value)` | `void` | Adds a value without replacing existing ones. |
| `set(name, value)` | `void` | Replaces every value for `name` with one value. |
| `get(name)` | `Option<string>` | First matching value, or `None`. |
| `get_all(name)` | `string[]` | Copy of every matching value in order. |
| `remove(name)` | `void` | Removes every value for `name`. |
| `to_json()` | `JsonValue` | Ordered array of `[name, value]` pairs. |

Native response headers preserve repeated values. Fetch can combine
repeated values and browser CORS rules can hide response headers. Node preserves
separate `Set-Cookie` values where its Fetch API exposes them.

## Responses

| Member | Returns | Effect |
|---|---|---|
| `status` | `int` | Numeric HTTP status. |
| `url` | `Url` | Final URL after redirects visible to the host. |
| `headers` | `HttpHeaders` | Response headers visible to the host. |
| `body` | `HttpBody` | Streamed body; implements `AsyncReader`. |
| `read_text(max_bytes, token)` | `async Result<string, HttpError>` | Reads the rest of the body as UTF-8; `max_bytes` defaults to 1048576. |
| `close()` | `Result<bool, HttpError>` | Closes the body; idempotent. |

`HttpBody` has `read_async(max_bytes, token)`, returning
`Result<byte[], IoError>`, and an idempotent `close()`.

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

`HttpOptions(timeout_ms, max_redirects, connect_timeout_ms)` accepts named
arguments for each field:

| Field | Default | Effect |
|---|---|---|
| `timeout_ms` | `30000` | Total deadline, including body consumption; `0` disables it. |
| `max_redirects` | `-1` | `-1` uses the host policy; `0` exposes redirect responses. |
| `connect_timeout_ms` | `0` | Connection timeout; `0` uses the host default. |

Negative deadlines and redirect limits below `-1` return `EINVAL` from `send`.
Native and Node host-default redirect policy follows at most ten redirects.
Explicit zero redirects exposes the redirect response; positive values enforce
a limit and return `EREDIRECT` when exceeded.

`CancellationToken` is accepted by `send`, `read_text`, and body reads. Cancellation
returns `ECANCELLED`, abandons pending transport work, and does not roll back
request side effects or consumed input. Overlapping body reads report `EBUSY`.
HTTP completion and cancellation wake the scheduler through notifications. No
periodic timer checks are needed while a request or body read is pending.

## Error codes

`HttpError` has `code()` for the stable category and `message()` for details.

| Code | Meaning |
|---|---|
| `EINVAL` | Invalid request, options, or argument. |
| `ETIMEOUT` | The deadline expired. |
| `EREDIRECT` | An explicit redirect limit was exceeded. |
| `ETRANSPORT` | Network or TLS failure. |
| `ECANCELLED` | A cancellation token fired. |
| `ECLOSED` | The client or body was already closed. |
| `ENOTSUP` | The host does not support a requested control or header. |
| `ELIMIT` | A body exceeded the `read_text` limit, or a host resource limit was reached. |
| `EBUSY` | Overlapping body read, or host capacity exhausted. |
| `EENCODING` | The body was not valid UTF-8. |
| `EIO` | The host returned malformed response data. |

Body reads expose corresponding `IoError` codes to compose with
other streams. Transport errors omit request URLs from their messages.

## Browser capabilities

Inspect `HttpCapabilities` before sending to see which controls the host enforces:

| Property | `true` when |
|---|---|
| `connection_timeouts` | A nonzero `connect_timeout_ms` is supported (native). |
| `redirect_limits` | Explicit redirect limits and redirect responses are observable (native, Node). |
| `unrestricted_headers` | Browser forbidden-header rules do not apply. |

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
    let request = HttpRequest(HttpMethod.Get, Url.parse("https://example.com/").unwrap());
    request.headers.append("X-Role", "reader");
    request.headers.append("x-role", "editor");
    System.println(request.headers.get_all("X-ROLE").length);
    let result = client.send(request, token: Option.Some(cancellation.token)).await;
    System.println(result.unwrap_err().code());
    return client.close();
}
```
