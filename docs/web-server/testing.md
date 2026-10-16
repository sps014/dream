---
title: Testing
summary: Test an app through the HTTP client on an isolated port.
icon: material/test-tube
---

# Testing an application

Bind to port zero and test through the public HTTP client. Each test creates an isolated app.

```dream
import system;
import system.webapi;
import system.net;
import system.testing;

@router("")
class Routes {
    @get("/health")
    public static fun health(): string {
        return "ok";
    }
}

@test
async fun health_test(): void {
    let app = WebApp();
    app.include_router(Routes.router()).unwrap();
    let port = app.listen("127.0.0.1", 0).await.unwrap();
    let client = HttpClient();
    let response = client.send(
        HttpRequest.get(Url.parse("http://127.0.0.1:" + port.to_string() + "/health").unwrap())
    ).await.unwrap();
    Assert.eq(response.status, 200);
    Assert.eq_str(response.read_text().await.unwrap(), "ok");
    response.close();
    app.shutdown();
    app.wait().await;
    client.close().unwrap();
}
```

Put the test in a Dream test package with `type = "test"` and native test targets, then run `dream test path/to/tests`. Use `@timeout` for network tests so a missing shutdown cannot hang the suite.

Override external dependencies with a generated replacement class and `app.override_dependencies(...)` before startup. Test both the overridden app and an ordinary app in the same process to detect configuration leaks.

## What to cover

| Behavior | Check |
| --- | --- |
| Input handling | Missing and malformed parameters return 422 with the expected `loc` and `type`. |
| Validation | Each constraint and custom validator rejects bad models. |
| Authentication | Requests without credentials get 401; requests without a scope get 403. |
| Middleware | Layers run in registration order and can short-circuit. |
| OpenAPI | `app.openapi()` contains the mounted paths and prefixes you expect. |
| Resources | Cleanup runs after a client disconnects mid-request. |
| Startup | A failing `lifespan` hook unwinds the hooks that already succeeded. |
| Session stores | Concurrent refresh consumption and replay revocation, not only sequential success. |

| Testing tool | Use it to |
| --- | --- |
| `@test` | Mark a test function. |
| `@timeout(ms)` | Fail a test that runs longer than `ms` milliseconds. |
| `app.listen("127.0.0.1", 0)` | Start on a free port and get its number. |
| `app.override_dependencies(...)` | Replace external services for this app only. |
| `app.shutdown()` then `app.wait()` | Stop the app before the test returns. |

HTTP server tests require the native target. Choose an unused port by passing zero to `listen`, and use the returned port in the client URL. Close each response and stop the application before the test returns.

See the [API lookup](../reference/api/system-webapi.md) and [complete example](example.md).
