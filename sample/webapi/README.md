# sample/webapi

Native FastAPI-style server (`dream run` only).

```bash
dream run sample/webapi/app.dream
```

- CORS (`CORS(CorsOptions())`, FastAPI-style)
- `GET /health` — plain text
- `GET /items/{id}` — JSON
- `POST /items` — JSON body
- [http://127.0.0.1:8080/docs](http://127.0.0.1:8080/docs) — Swagger UI
- [http://127.0.0.1:8080/openapi.json](http://127.0.0.1:8080/openapi.json) — OpenAPI 3
