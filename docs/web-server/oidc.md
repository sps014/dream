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
        JwtOptions("https://api.example.com", "example-api", key, key, JwtAlgorithm.HS256),
        Accounts(""),
        MemoryAuthStore()
    );
    auth.add_provider(
        OidcOptions(
            "company",
            System.env("OIDC_ISSUER").unwrap(),
            System.env("OIDC_CLIENT_ID").unwrap(),
            "https://api.example.com/auth/oidc/company/callback",
            System.env("OIDC_CLIENT_SECRET").unwrap_or("")
        )
    ).unwrap();
    let app = WebApp();
    app.include_router(auth.router().unwrap()).unwrap();
    app.use(auth.middleware()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

Register the exact callback URI with the provider, then set the issuer URL and client ID. Public clients omit the client secret and confidential clients supply it on the server; the sample `Accounts` class rejects linking until you add your identity policy.

| `OidcOptions` argument | Default | Meaning |
| --- | --- | --- |
| `name` | Required | Provider name used in the route paths, such as `company`. |
| `issuer` | Required | The provider's issuer URL; its discovery document is loaded from here. |
| `client_id` | Required | The client ID registered with the provider. |
| `redirect_uri` | Required | The exact callback URL registered with the provider. |
| `client_secret` | `""` | Only for confidential clients. Leave empty for public clients. |

`auth.add_provider(options)` registers a provider and adds two routes for it:

| Route | Effect |
| --- | --- |
| `GET /auth/oidc/{name}` | Starts login and redirects to the provider. |
| `GET /auth/oidc/{name}/callback` | Completes login, links the identity, and returns tokens. |

GET `/auth/oidc/company` starts login: the service discovers the provider, stores state, a nonce, and an S256 PKCE challenge, sets a browser-binding cookie, and redirects. The callback checks that cookie and state, consumes the transaction once, and then exchanges the code.

Login checks the issuer, audience, signature, expiry, nonce, and access-token hash when the provider sends one. Discovery and token exchange do not follow HTTP redirects, and HTTPS is required except on loopback URLs.

`AuthAccounts.link_identity` receives the verified issuer and subject, plus the email and whether it was verified. Use issuer and subject as the external identity key, and do not link an account from an email match alone.

Successful login returns tokens and sets a `Secure; HttpOnly; SameSite=Lax` host-only session cookie. Cookie writes need an `X-CSRF-Token` that matches the session, the callback transaction expires after ten minutes and cannot be reused, and browser sessions need TLS.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
