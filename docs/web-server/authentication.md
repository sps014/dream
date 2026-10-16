---
title: Local authentication
summary: "Passwords, signed access tokens, and refresh sessions."
icon: material/account-key-outline
---

# Local authentication

Use `system.webapi.auth` for Argon2id passwords, signed access tokens, and rotating refresh sessions. The account repository and persistent storage remain application-owned.

```dream
import system;
import system.webapi;
import system.webapi.auth;

class Accounts: AuthAccounts {
    hash: string;

    public constructor(hash: string) {
        this.hash = hash;
    }

    public async unique fun find_login(login: string): Result<AuthAccount, AuthError> {
        if login != "demo" {
            return Result.Err(AuthError("EAUTH", "unknown account"));
        }
        return Result.Ok(AuthAccount("demo-user", this.hash, "read"));
    }

    public async unique fun link_identity(
        issuer: string,
        subject: string,
        email: string,
        email_verified: bool
    ): Result<AuthAccount, AuthError> {
        return Result.Err(
            AuthError("EAUTH", "external account linking requires application policy")
        );
    }
}

async fun main(): void {
    let key = System.env("API_JWT_KEY").unwrap();
    let password = System.env("DEMO_PASSWORD").unwrap();
    let auth = AuthService(
        JwtOptions("https://api.example.com", "example-api", key, key, "HS256"),
        Accounts(Password.hash(password).unwrap()),
        MemoryAuthStore()
    );
    let app = WebApp();
    app.include_router(auth.router().unwrap()).unwrap();
    app.use(auth.middleware()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Set `API_JWT_KEY` to an explicit secret containing at least 32 bytes and `DEMO_PASSWORD` to a chosen demonstration password. Do not use this in-memory repository for production accounts.

POST JSON `{"username":"demo","password":"..."}` to `/auth/login`. The response contains `access_token`, `refresh_token`, `token_type`, `expires_in`, and `csrf_token`. Access tokens default to 900 seconds and refresh sessions to 604800 seconds. Send an access token as `Authorization: Bearer <token>`.

POST `{"refresh_token":"..."}` to `/auth/refresh` to rotate the refresh token. Each token can be consumed once. Replay revokes the entire session family, including already-issued access tokens, because verification checks the backing session's active state. POST a refresh token to `/auth/logout` to revoke its family.

`JwtOptions` defaults to RS256 with separate signing/private and verification/public PEM keys. HS256 is an explicit option. Issuer, audience, algorithm, signature, expiry, and session activity are verified. Configuration requires explicit keys; the framework never creates an application signing key.

`MemoryAuthStore` is for tests and single-process examples. Implement `AuthSessionStore` for production persistence. `rotate` must atomically consume the old digest, insert its replacement, and revoke the family if the old digest was consumed. A read followed by a separate write is insufficient under concurrency. Store refresh digests rather than bearer secrets.

Passwords use Argon2id PHC strings. Verification returns an error for malformed or unsupported hash parameters and false for a password mismatch. The crypto host bounds password and PHC costs. Protect credential endpoints with deployment-appropriate request limits.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).

To issue local browser session cookies, set `browser_sessions: true` and an explicit `browser_origin`. Browser login requests must send that exact Origin; subsequent cookie-authenticated writes must send the session CSRF token.
