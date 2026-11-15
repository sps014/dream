# JwtOptions

**Import:** `import system.webapi.auth;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JwtOptions`

Native JWT configuration; RS256 uses PEM keys, HS256 requires an explicit 32-byte minimum secret.

```dream
public class JwtOptions
```

## `issuer`

Identity of the token issuer.

```dream
public issuer: string
```

## `audience`

Expected token audience.

```dream
public audience: string
```

## `signing_key`

Private key used to sign tokens; keep it secret.

```dream
public signing_key: string
```

## `verification_key`

Public key used to verify token signatures.

```dream
public verification_key: string
```

## `algorithm`

Signature algorithm name, such as `RS256`.

```dream
public algorithm: JwtAlgorithm
```

## `constructor`

Creates a `JwtOptions` with `issuer`, `audience`, `signing_key`, `verification_key`, `algorithm`.

```dream
public constructor(
    issuer: string,
    audience: string,
    signing_key: string,
    verification_key: string,
    algorithm: JwtAlgorithm = JwtAlgorithm.RS256
)
```
