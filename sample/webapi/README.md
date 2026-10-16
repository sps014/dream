# Native Web API sample

Run from the repository root:

```sh
cargo run -- run sample/webapi/app.dream
```

Open `http://127.0.0.1:8000/docs` for Swagger UI, `/redoc` for ReDoc,
`/hello/Dream` for JSON, or `/events` for server-sent events. The docs viewers load
scripts from a CDN. This server requires the native `dream-host-net` capability;
WebAssembly builds are rejected at compile time.

The [service example](service/) adds multiple files, validation, local authentication, scopes, dependencies, CORS, and a login test. Set explicit environment credentials before running it; see [the complete guide](../../docs/web-server/example.md).
