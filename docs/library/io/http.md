---
title: HTTP
summary: Send HTTP requests and stream responses.
icon: material/web
api: reference/api/system-net.md
---

# HTTP clients and streamed responses

Import `system.net` for HTTP clients, and `system.io` when copying a response to a file.

```dream
import system;
import system.net;

async fun main(): Result<Unit, HttpError> {
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

Reuse one client for many requests. `send` copies the URL, headers, options, and body when it starts, and a 4xx or 5xx status is still a successful response.

## Client

| Member | Returns | Effect |
|---|---|---|
| `HttpClient(use_cookies: true, cookies: None)` | `HttpClient` | Creates a client with an isolated cookie jar on native hosts; allocation failures surface from `send`. |
| `cookies` | `Option<CookieJar>` | The default or supplied jar, when available. |
| `send(request, token)` | `async Result<HttpResponse, HttpError>` | Sends with default `HttpOptions()`. |
| `send(request, options, token)` | `async Result<HttpResponse, HttpError>` | Sends with explicit options; completes when headers arrive. |
| `close()` | `Result<Unit, HttpError>` | Cancels pending sends and releases pools; idempotent. |

`token` is an optional `Option<CancellationToken>` and defaults to `None`.

## Cookies

A native client stores `Set-Cookie` values and sends matching cookies later, including after redirects. Each default client has its own in-memory jar, and cookies are not written to disk.

`HttpClient(use_cookies: false)` turns storage and sending off.

Supply a `CookieJar` to seed cookies or share a session between clients:

```dream
import system;
import system.net;

async fun main(): Result<Unit, HttpError> {
    let url = Url.parse("https://example.com/").unwrap();
    if !HttpCapabilities.cookie_jars {
        return Result.Err(HttpError("ENOTSUP", "this example requires native cookie support"));
    }

    let jar = CookieJar();
    jar.store(url, "language=en; Path=/; Secure")?;
    let client = HttpClient(cookies: Option.Some(jar));
    let response = client.send(HttpRequest(HttpMethod.Get, url)).await?;
    let body = response.read_text().await;
    response.close()?;
    client.close()?;
    System.println(body?);

    jar.clear()?;
    return Result.Ok(Unit());
}
```

`store(url, set_cookie)` applies one cookie from that URL, including replacement, `Max-Age`, and `Expires`.

- A bad cookie, or one for an unrelated or public-suffix domain, is ignored. Bad header bytes or a non-HTTP URL return `EINVAL`.
- Store each `Set-Cookie` value on its own. Do not join them with commas.
- `header(url)` returns the `Cookie` header to send, or `""`. Selection follows host, path, expiry, and `Secure`; longer paths come first, and equal paths keep creation order.
- `__Secure-` requires HTTPS and `Secure`. `__Host-` also requires path `/` and no `Domain`.

An explicit `Cookie` header wins over the jar. Response cookies are still stored.

- A cross-origin redirect drops that header and `Authorization`. The jar is then chosen for the new URL.
- Clearing a shared jar affects every client that uses it. A later response can store new cookies, and closing one client does not clear the jar.

Node, browsers, and WASI components have no managed jar. Default clients there stay stateless.

Passing an enabled jar makes `send` and jar methods return `ENOTSUP`. A jar passed with `use_cookies: false` stays readable and is not used.

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

Native response headers keep repeated values. Fetch may combine them, and browser CORS can hide some; Node keeps separate `Set-Cookie` values when Fetch exposes them.

## Responses

| Member | Returns | Effect |
|---|---|---|
| `status` | `int` | Numeric HTTP status. |
| `url` | `Url` | Final URL after redirects visible to the host. |
| `headers` | `HttpHeaders` | Response headers visible to the host. |
| `body` | `HttpBody` | Streamed body; implements `AsyncReader`. |
| `read_text(max_bytes, token)` | `async Result<string, HttpError>` | Reads the rest of the body as UTF-8; `max_bytes` defaults to 1048576. |
| `close()` | `Result<Unit, HttpError>` | Closes the body; idempotent. |

`HttpBody` has `read_async(max_bytes, token)`, returning
`Result<byte[], IoError>`, and an idempotent `close()`.

## Streaming and cleanup

`send` finishes when the response headers arrive. `response.body` is an `AsyncReader`: it returns up to the size you ask for, and an empty read means EOF.

Use `Streams.copy_async(response.body, destination)` to download without holding the whole body. Native keeps one transport chunk plus your output buffer; Fetch keeps one received chunk between reads.

`read_text` decodes UTF-8 as it goes, with a default limit of 1 MiB, and reads one extra byte to notice that limit. An encoding or limit error does not put consumed bytes back, so close the response after an error.

Close every response, including a body you did not finish. `close()` aborts a pending read and is safe to call again.

A `body` you kept is separate from the response value, and closing either one closes that body everywhere. Dropping a client or an unfinished body releases it without throwing.

`client.close()` cancels sends that have not finished and releases connection pools. Responses you already have stay usable.

Native clients keep at most 32 combinations of connection timeout and redirect policy. Each host allows 1024 clients, 1024 bodies, and 1024 operation records; past that, calls return `EBUSY` or `ELIMIT`.

## Deadlines, redirects, and cancellation

`HttpOptions(timeout_ms, max_redirects, connect_timeout_ms)` accepts named
arguments for each field:

| Field | Default | Effect |
|---|---|---|
| `timeout_ms` | `30000` | Total deadline, including body consumption; `0` disables it. |
| `max_redirects` | `-1` | `-1` uses the host policy; `0` exposes redirect responses. |
| `connect_timeout_ms` | `0` | Connection timeout; `0` uses the host default. |

A negative deadline, or a redirect limit below `-1`, returns `EINVAL` from `send`.

With the default, native and Node follow at most ten redirects. `max_redirects: 0` returns the redirect response, and a positive limit returns `EREDIRECT` when it is exceeded.

`send`, `read_text`, and body reads take a `CancellationToken`. Cancellation returns `ECANCELLED`, drops transport work still in progress, and does not undo a request that already started or bytes already read.

Overlapping body reads return `EBUSY`.

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
| `cookie_jars` | Managed in-memory cookie jars are supported (native). |

Fetch rejects a nonzero `connect_timeout_ms`. Browsers also require `max_redirects: -1`, because the browser follows redirects and does not reliably show them; native and Node accept an explicit limit.

Browsers reject restricted request headers with `ENOTSUP`, including connection, cookie, origin, host, content-length, and `Sec-` or `Proxy-` names.

CORS, mixed content, and the browser's certificate rules still apply. An opaque response returns `ENOTSUP`, Fetch does not send ambient credentials, and nothing turns certificate checks off.

Closing a Node client does not drain Node's connection pool. Node owns that pool.

## Cancellation before sending

A token cancelled before `send` returns `ECANCELLED` without issuing a request.
This example runs without network access:

```dream
import system;
import system.net;

async fun main(): Result<Unit, HttpError> {
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
