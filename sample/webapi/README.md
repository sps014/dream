# sample/webapi

This example runs a local HTTP service on your computer. It shows routes, JSON request bodies, grouped handlers, cross-origin settings, and generated API documentation. Run it from the repository root using the command below.

## About this example

This is a desktop HTTP server.

```bash
dream run sample/webapi/app.dream
```

- CORS (`CORS(CorsOptions())`)
- `GET /health` — plain text
- `GET /api/items/{id}` — JSON (`@http_group`)
- `POST /api/items` — JSON body
- [http://127.0.0.1:8080/docs](http://127.0.0.1:8080/docs) — Swagger UI
- [http://127.0.0.1:8080/redoc](http://127.0.0.1:8080/redoc) — ReDoc
- [http://127.0.0.1:8080/openapi.json](http://127.0.0.1:8080/openapi.json) — OpenAPI 3
