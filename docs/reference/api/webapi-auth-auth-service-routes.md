# AuthService routes

**Import:** `import system.webapi.auth;`

## `router`

Generates the auth endpoints; mount the result explicitly on one application.

```dream
public fun router(): Result<WebRouter, AuthError>
```
