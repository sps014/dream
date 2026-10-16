# Complete service example

The repository's [`sample/webapi/service/`](https://github.com/sps014/dream/tree/main/sample/webapi/service) combines application configuration, explicit routers, CORS, validated JSON models, authentication, scope checks, dependency injection, and an HTTP test.

## Files

- `models.dream` declares the `@json` request model and field constraints.
- `accounts.dream` implements local account lookup and rejects external linking until application policy is supplied.
- `routes.dream` declares the `/messages` router, its identity/scope dependency, and a request-label provider.
- `application.dream` builds an app from explicit configuration and mounts only the intended routers.
- `app.dream` reads environment configuration and runs the service.
- `service_tests.dream` creates an isolated app and exercises login over HTTP.

## Run it

Set `API_JWT_KEY` to at least 32 bytes and `DEMO_PASSWORD` to your demonstration password, then run from the repository root:

```sh
dream run sample/webapi/service/app.dream
```

Open `/docs`. POST JSON credentials with username `demo` to `/auth/login`, copy the returned access token into Swagger's Bearer Authorize control, then call `POST /messages` with `{"text":"Hello"}`. An empty message returns 422. A missing access token returns 401. A user without `messages:write` receives 403.

The sample uses HS256 explicitly. Production services may use the RS256 default with separate PEM keys. The example's account lookup and `MemoryAuthStore` are deliberately process-local; replace them with your application's repositories and atomic persistent session store.

## Test it

```sh
dream test sample/webapi/service/service_tests.dream
```

The test supplies explicit non-production keys and credentials, binds an ephemeral port, closes the HTTP response, and shuts the app down before returning. No external identity provider is required for this local test.

For external login, configure `OidcOptions` and implement `AuthAccounts.link_identity` according to your account policy. See [OAuth2/OIDC](oidc.md) for callback URLs, transaction storage, and browser cookies.

See the [API lookup](../reference/api/system-webapi.md) for public signatures and [testing guide](testing.md) for dependency overrides and failure cases.
