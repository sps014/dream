# CookieJar

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class CookieJar`

In-memory cookie store for `HttpClient` (no OS cookie jar).

```dream
public class CookieJar
```

## `constructor`

Creates an empty jar.

```dream
public constructor()
```

## `set`

Stores or overwrites cookie `name`.

```dream
public fun set(name: string, value: string): void
```

## `get`

Value for `name`, or `None` when absent.

```dream
public fun get(name: string): Option<string>
```

## `remove`

```dream
public fun remove(name: string): bool
```

## `clear`

Removes every cookie.

```dream
public fun clear(): void
```

## `length`

Number of stored cookies.

```dream
public get length(): int
```

## `to_header`

Builds a `Cookie` request header value (`name=value; …`).

```dream
public fun to_header(): string
```

## `store_from_response`

Parses `Set-Cookie` lines from an HTTP response into this jar.

```dream
public fun store_from_response(res: HttpResponse): void
```
