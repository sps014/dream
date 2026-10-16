# Http Types

**Import:** `import system.net;`

## `get`

Creates a GET request.

```dream
public static fun get(url: Url): HttpRequest
```

## `post`

Creates a POST request.

```dream
public static fun post(url: Url): HttpRequest
```

## `put`

Creates a PUT request.

```dream
public static fun put(url: Url): HttpRequest
```

## `patch`

Creates a PATCH request.

```dream
public static fun patch(url: Url): HttpRequest
```

## `delete`

Creates a DELETE request.

```dream
public static fun delete(url: Url): HttpRequest
```

## `set_text`

Replaces the body with UTF-8 bytes; adds no content-type header implicitly.

```dream
public fun set_text(text: string): void
```

## `HttpCapabilities`

Host-enforced HTTP controls; unsupported options still return `ENOTSUP` at send time.

```dream
public static class HttpCapabilities
```

## `connection_timeouts`

Whether a separate connection-establishment timeout can be configured (native hosts).

```dream
public static get connection_timeouts(): bool
```

## `redirect_limits`

Whether explicit redirect limits and redirect responses are observable (native and Node).

```dream
public static get redirect_limits(): bool
```

## `unrestricted_headers`

Whether browser forbidden-header restrictions do not apply; HTTP syntax is always validated.

```dream
public static get unrestricted_headers(): bool
```
