---
title: Complete example
summary: A multi-file service combining every piece.
icon: material/puzzle-outline
---

# Complete service example

This service combines application configuration, explicit routers, CORS, validated JSON models, authentication, scope checks, dependency injection, and an HTTP test.

## Create the project

Create an empty directory and save this configuration as `dream.toml`:

```toml
[package]
name = "webapi_service"
version = "0.1.0"
entry = "app.dream"
```

Save the following files alongside it. The account lookup is for a local demonstration:
it accepts only the `demo` account and does not link external identities.

### `models.dream`

```dream
module service.models;
import system.json;
import system.validation;

@json
public class CreateMessage {
    @min_length(1)
    @max_length(200)
    public text: string;

    public constructor(text: string) {
        this.text = text;
    }
}
```

### `accounts.dream`

```dream
module service.accounts;
import system.webapi.auth;

public class DemoAccounts: AuthAccounts {
    hash: string;

    public constructor(hash: string) {
        this.hash = hash;
    }

    public async unique fun find_login(login: string): Result<AuthAccount, AuthError> {
        if login != "demo" {
            return Result.Err(AuthError("EAUTH", "invalid account"));
        }
        return Result.Ok(AuthAccount("demo", this.hash, "messages:write"));
    }

    public async unique fun link_identity(
        issuer: string,
        subject: string,
        email: string,
        email_verified: bool
    ): Result<AuthAccount, AuthError> {
        return Result.Err(
            AuthError("EAUTH", "external identities require explicit account linking")
        );
    }
}
```

### `routes.dream`

```dream
module service.routes;
import system.webapi;
import system.webapi.auth;
import models;

public fun request_label(@header("X-Request-ID")label: Option<string>): string {
    return label.unwrap_or("local");
}

@router("/messages")
public class Messages {
    @requires_scopes("messages:write")
    @operation("Create a message", "Returns the validated message", "messages", 201)
    @post("/")
    public static fun create(
        @dep(CurrentUser)user: AuthPrincipal,
        @dep(request_label)label: string,
        @body message: CreateMessage
    ): HttpOutgoing {
        return HttpOutgoing.json_text(Json.serialize(message), 201)
            .set_header("X-Request-ID", label)
            .set_header("X-User", user.subject);
    }
}
```

### `application.dream`

```dream
module service.application;
import system;
import system.webapi;
import system.webapi.auth;
import accounts;
import routes;

public fun create_app(key: string, password: string): Result<WebApp, AuthError> {
    let auth = AuthService(
        JwtOptions("dream-example", "messages-api", key, key, "HS256"),
        DemoAccounts(Password.hash(password)?),
        MemoryAuthStore()
    );
    let app = WebApp(
        WebAppOptions(
            title: "Messages API",
            description: "Validated messages with dependency-based authentication"
        )
    );
    let mounted = app.include_router(auth.router()?);
    if mounted.is_err() {
        return Result.Err(AuthError("ECONFIG", mounted.unwrap_err().message()));
    }
    app.include_router(Messages.router()).unwrap();
    app.use(auth.middleware()).unwrap();
    app.use(
        CORS(CorsOptions(allow_origins: "https://example.com", expose_headers: "X-Request-ID"))
    ).unwrap();
    return Result.Ok(app);
}
```

### `app.dream`

```dream
import system;
import application;

async fun main(): void {
    let key = System.env("API_JWT_KEY");
    let password = System.env("DEMO_PASSWORD");
    if key.is_none() || password.is_none() {
        System.println("Set API_JWT_KEY (at least 32 bytes) and DEMO_PASSWORD");
        return;
    }
    let configured = create_app(key.unwrap(), password.unwrap());
    if configured.is_err() {
        System.println(configured.unwrap_err().message);
        return;
    }
    configured.unwrap().run("127.0.0.1", 8000).await.unwrap();
}
```

## Run it

Set `API_JWT_KEY` to at least 32 bytes and `DEMO_PASSWORD` to your demonstration password, then run from the project directory:

```sh
dream run app.dream
```

Open `/docs`. POST JSON credentials with username `demo` to `/auth/login`, copy the returned access token into Swagger's Bearer Authorize control, then call `POST /messages` with `{"text":"Hello"}`. An empty message returns 422. A missing access token returns 401. A user without `messages:write` receives 403.

This example uses HS256 explicitly. Production services may use the RS256 default with separate PEM keys. The example's account lookup and `MemoryAuthStore` are deliberately process-local; replace them with your application's repositories and atomic persistent session store.

## Test it

Save this as `service_tests.dream` in the same directory:

```dream
import system;
import system.net;
import system.testing;
import application;

@test
@timeout(10000)
async fun service_login(): void {
    let app = create_app("0123456789abcdef0123456789abcdef", "example-password").unwrap();
    let port = app.listen("127.0.0.1", 0).await.unwrap();
    let client = HttpClient();
    let request = HttpRequest(
        "POST",
        Url.parse("http://127.0.0.1:" + port.to_string() + "/auth/login").unwrap()
    );
    request.headers.set("Content-Type", "application/json");
    request.body = Encoding.utf8_encode(
        "{\"username\":\"demo\",\"password\":\"example-password\"}"
    );
    let response = client.send(request).await.unwrap();
    Assert.eq(response.status, 200);
    response.close();
    app.shutdown();
    app.wait().await;
    client.close().unwrap();
}
```

Run `dream test service_tests.dream`.

The test supplies explicit non-production keys and credentials, binds an ephemeral port, closes the HTTP response, and shuts the app down before returning. No external identity provider is required for this local test.

For external login, configure `OidcOptions` and implement `AuthAccounts.link_identity` according to your account policy. See [OAuth2/OIDC](oidc.md) for callback URLs, transaction storage, and browser cookies.

See the [API lookup](../reference/api/system-webapi.md) for public signatures and [testing guide](testing.md) for dependency overrides and failure cases.
