---
title: OAuth2 and OIDC
summary: Sign users in with an external identity provider.
icon: material/account-switch-outline
---

# External OAuth2 and OIDC login

External login uses Authorization Code + PKCE and verified OIDC identity claims. Account linking is an explicit repository decision.

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
    let auth = AuthService(
        JwtOptions("https://api.example.com", "example-api", key, key, "HS256"),
        Accounts(""),
        MemoryAuthStore()
    );
    auth.add_provider(
        OidcOptions(
            "company",
            System.env("OIDC_ISSUER").unwrap(),
            System.env("OIDC_CLIENT_ID").unwrap(),
            "https://api.example.com/auth/oidc/company/callback",
            System.env_or("OIDC_CLIENT_SECRET", "")
        )
    ).unwrap();
    let app = WebApp();
    app.include_router(auth.router().unwrap()).unwrap();
    app.use(auth.middleware()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Register the exact callback URI with the provider, then configure the issuer URL and client ID. Public clients omit the client secret; confidential clients supply it server-side. The demonstration `Accounts` class rejects linking until you implement your application's identity policy.

GET `/auth/oidc/company` starts login. The service discovers provider endpoints, generates state, nonce and an S256 PKCE challenge, stores an expiring transaction, sets a browser-binding cookie, and redirects to the provider. The callback verifies the cookie/state binding and atomically consumes the transaction before exchanging the code.

The native OIDC library validates the issuer, audience, signature, expiry, nonce, and access-token hash when present. HTTP redirects during discovery and token exchange are disabled. HTTPS is required except for loopback development URLs.

`AuthAccounts.link_identity` receives verified issuer/subject plus email and its verification flag. Use issuer and subject as the external identity key. Email equality alone must not silently link an external identity to an existing local account.

Successful login returns tokens and sets a `Secure; HttpOnly; SameSite=Lax` host-only session cookie. Authenticated cookie writes require `X-CSRF-Token` matching the session's CSRF value. The callback transaction expires after ten minutes and cannot be reused. Configure TLS before using browser sessions.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
