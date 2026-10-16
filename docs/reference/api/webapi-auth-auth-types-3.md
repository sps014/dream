# Auth Types

**Import:** `import system.webapi.auth;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public name`

```dream
public name: string
```

## `public issuer`

```dream
public issuer: string
```

## `public client_id`

```dream
public client_id: string
```

## `public client_secret`

```dream
public client_secret: string
```

## `public redirect_uri`

```dream
public redirect_uri: string
```

## `public constructor`

```dream
public constructor(name: string, issuer: string, client_id: string, redirect_uri: string, client_secret: string = "")
```

## `public class AuthTokens`

```dream
public class AuthTokens
```

## `public access_token`

```dream
public access_token: string
```

## `public refresh_token`

```dream
public refresh_token: string
```

## `public token_type`

```dream
public token_type: string
```

## `public expires_in`

```dream
public expires_in: int
```

## `public csrf_token`

```dream
public csrf_token: string
```

## `public constructor`

```dream
public constructor(access_token: string, refresh_token: string, token_type: string, expires_in: int, csrf_token: string)
```
