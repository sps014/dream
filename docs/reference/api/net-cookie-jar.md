# Cookie Jar

**Import:** `import system.net;`

## `CookieJar`

In-memory HTTP cookies shared by one or more native clients. Cookies are never saved to disk. Node, browsers, and WASI components do not expose managed jars; operations return ENOTSUP there.

```dream
public class CookieJar
```

## `constructor`

Creates an empty jar. Resource exhaustion is reported by its operations and client sends.

```dream
public constructor()
```

## `store`

Processes one Set-Cookie value as received from `url`, including replacement and expiry. Invalid cookies, unrelated/public-suffix domains, and invalid secure prefixes are ignored. Invalid header bytes or URLs return EINVAL.

```dream
public fun store(url: Url, set_cookie: string): Result<Unit, HttpError>
```

## `header`

Returns the Cookie header for `url`, or an empty string when no unexpired cookies match. Selection respects host/domain, path, and Secure; HTTP URLs containing credentials return EINVAL. Longer paths come first; equal paths retain creation order when cookies are replaced.

```dream
public fun header(url: Url): Result<string, HttpError>
```

## `clear`

Removes all cookies, including session cookies, for every client sharing this jar. Responses arriving after this call can store new cookies.

```dream
public fun clear(): Result<Unit, HttpError>
```
