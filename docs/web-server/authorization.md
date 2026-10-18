---
title: Authorization and scopes
summary: Require permission scopes on protected handlers.
icon: material/shield-account-outline
---

# Authorization and scopes

Authentication establishes identity. A protected handler injects `CurrentUser` and can require permission scopes.

```dream
import system;
import system.webapi;
import system.webapi.auth;

@router("/reports")
class Routes {
    @requires_scopes("read")
    @get("/")
    public static fun reports(@dep(CurrentUser) user: AuthPrincipal): string {
        return "Reports for " + user.subject;
    }
}

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
        JwtOptions("issuer", "reports", key, key, "HS256"),
        Accounts(Password.hash(System.env("DEMO_PASSWORD").unwrap()).unwrap()),
        MemoryAuthStore()
    );
    let app = WebApp();
    app.include_router(auth.router().unwrap()).unwrap();
    app.include_router(Routes.router()).unwrap();
    app.use(auth.middleware()).unwrap();
    app.run("127.0.0.1", 8000).await.unwrap();
}
```

| Name | Kind | Effect |
| --- | --- | --- |
| `@dep(CurrentUser)` | Dependency | Injects the `AuthPrincipal` that `AuthService.middleware()` verified. Returns 401 when there is none. |
| `@requires_scopes("read write")` | Route attribute | Requires every listed scope. Returns 403 if any is missing. Also listed in OpenAPI. |
| `AuthPrincipal.subject` | Field | The account's stable identifier. |
| `AuthPrincipal.scopes` | Field | The granted scopes, space-separated. |
| `AuthPrincipal.has_scope(scope)` | Method | Whether one scope is granted. |

Scope strings are assigned by the account repository (`AuthAccount.scopes`) and included in issued tokens.

Keep authorization requirements on the route that injects `CurrentUser`. Merely adding a metadata attribute does not authenticate a request. The generated security scheme adds Swagger Authorize support; actual enforcement occurs in the middleware and dependency.

`AuthPrincipal.has_scope` checks an individual permission. For resource-specific policy, define another dependency that injects `CurrentUser`, loads the resource, checks ownership, and returns `Result<Resource, HttpStatus>`. Avoid accepting an identity or permission set from request headers without verification.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
