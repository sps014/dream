# Server Config

**Import:** `import system.webapi;`

For behavior and examples, see [Server configuration](../../web-server/configuration.md).

## `ServerConfig`

Transport settings for `WebApp.listen` and `WebApp.run`, named after Uvicorn's options. A zero limit or timeout means "none".

```dream
public class ServerConfig
```

## `ssl_certfile`

Path to the PEM server certificate; leave empty to serve plain HTTP.

```dream
public ssl_certfile: string
```

## `ssl_keyfile`

Path to the PEM private key for `ssl_certfile`.

```dream
public ssl_keyfile: string
```

## `limit_concurrency`

When more connections are open or more requests are in flight than this, new requests receive `503 Service Unavailable`. Zero accepts everything.

```dream
public limit_concurrency: int
```

## `backlog`

Maximum number of connections the operating system queues before the server accepts them.

```dream
public backlog: int
```

## `timeout_keep_alive_ms`

Milliseconds an idle keep-alive connection, or a client still sending its request headers, may stay silent before the server closes it. Zero waits forever.

```dream
public timeout_keep_alive_ms: int
```

## `timeout_graceful_shutdown_ms`

Milliseconds `shutdown` waits for in-flight requests before closing their connections. Zero waits until every request finishes.

```dream
public timeout_graceful_shutdown_ms: int
```

## `max_header_bytes`

Largest request head (request line plus headers) in bytes; larger requests are rejected with `431`. Must be at least 8192.

```dream
public max_header_bytes: int
```

## `ws_max_size`

Largest WebSocket message in bytes; a larger message closes the socket with code 1009. Zero removes the limit.

```dream
public ws_max_size: int
```

## `ws_max_queue`

Inbound WebSocket messages buffered per socket before the server stops reading from it.

```dream
public ws_max_queue: int
```

## `ws_ping_interval_ms`

Milliseconds between server pings on each WebSocket. Zero disables pings.

```dream
public ws_ping_interval_ms: int
```

## `ws_ping_timeout_ms`

Milliseconds the client has to answer a ping before the socket closes with code 1011.

```dream
public ws_ping_timeout_ms: int
```

## `proxy_headers`

When true, requests from a peer listed in `forwarded_allow_ips` take their client address from `X-Forwarded-For` and their scheme from `X-Forwarded-Proto`.

```dream
public proxy_headers: bool
```

## `forwarded_allow_ips`

Comma-separated addresses and CIDR networks of trusted proxies, such as `"127.0.0.1,10.0.0.0/8"`. `"*"` trusts every peer; use it only behind a proxy that overwrites these headers.

```dream
public forwarded_allow_ips: string
```

## `h2_max_concurrent_streams`

Most requests one HTTP/2 connection may have in progress at once. Must be at least 1.

```dream
public h2_max_concurrent_streams: int
```

## `constructor`

Creates a `ServerConfig`; every default matches Uvicorn's.

```dream
public constructor(
    ssl_certfile: string = "",
    ssl_keyfile: string = "",
    limit_concurrency: int = 0,
    backlog: int = 2048,
    timeout_keep_alive_ms: int = 5000,
    timeout_graceful_shutdown_ms: int = 0,
    max_header_bytes: int = 16384,
    ws_max_size: int = 16777216,
    ws_max_queue: int = 32,
    ws_ping_interval_ms: int = 20000,
    ws_ping_timeout_ms: int = 20000,
    proxy_headers: bool = true,
    forwarded_allow_ips: string = "127.0.0.1",
    h2_max_concurrent_streams: int = 100
)
```
