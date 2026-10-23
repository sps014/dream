# HttpCapabilities

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `HttpCapabilities`

Host-enforced HTTP controls; unsupported options still return `ENOTSUP` at send time.

```dream
public static class HttpCapabilities
```

## `cookie_jars`

Whether clients can automatically store and send cookies using a managed CookieJar (native hosts).

```dream
public static get cookie_jars(): bool
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
