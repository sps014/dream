# HTTP

**Import:** `import system.net;`

```dream
import system;
import system.net;

async fun main(): void {
    let api = HttpClient("https://api.example.com");
    switch (await api.get("/health")) {
        Ok(res) => System.println(res.status),
        Err(e) => System.println(e.message()),
    }
}
```

Native uses the host HTTP client (reqwest in `libdream`); browser and Node use `fetch`. Buffered `get`/`post` and streaming `get_stream`/`request_stream` share that path. Calls are `async` and return `Result`. To **serve** HTTP, see [`system.webapi`](webapi.md) (`WebApp`). A convenient public mock API for experiments is [JSONPlaceholder](https://jsonplaceholder.typicode.com/guide/).

## Client

| Call | Meaning |
| --- | --- |
| `HttpClient()` / `HttpClient(base_url)` | empty base by default; pass a prefix when paths are relative |
| `set_header(name, value)` | default header (chains) |
| `with_timeout(ms)` | `0` = none |
| `with_http_version(1 or 2)` | native only |
| `with_cookie_jar(jar)` | cookies |
| `with_cancellation(token)` | client-wide default token when a per-call token is omitted |
| `await text(path)` / `get_bytes(path)` | GET, body as string / bytes; optional last `token` |
| `await get` / `get_with` / `get_json` | full response; optional last `token` |
| `await post` / `post_with` / `post_json` / `post_form(path, Map<string,string>)` | POST; `_json` sends `application/json`, `_form` sends percent-encoded `application/x-www-form-urlencoded` |
| `await put` / `put_with` / `put_json` / `patch` / `patch_with` / `patch_json` | PUT/PATCH; `_json` variants send `application/json` |
| `await delete` / `delete_with` / `head` | DELETE/HEAD |
| `await request(method, path, body, headers)` | custom |
| `await post_bytes` / `put_bytes` / `request_bytes` | binary body |
| `await post_multipart(path, form)` | multipart |
| `await get_stream` / `request_stream` | chunked body |

Per-call headers override client defaults from `set_header` on a name collision; all other defaults are always sent. Every request method accepts `token: Option<CancellationToken> = None`; cancelled calls return `HttpError` with code `ECANCELLED` without hitting the network.

## Response and helpers

`HttpResponse`: `status()`, `ok()`, `header(name)`, `headers()`, `text()`, `bytes()`, `json()`.

`HttpHeaders`: `set` / `get` / `contains` / `remove` / `.length` / `to_wire` / `from_wire`.

`CookieJar`: `set` / `get` / `clear` / `to_header` / `store_from_response`.

`HttpStreamResponse`: `read_chunk`, `read_all`, `close`.

`MultipartForm`: `add_field` / `add_file` / `build()`.

`Url.parse`, `to_string()`, `with_path`, `join`.

Pair with [JSON](json.md). Example: [`sample/interop/http.dream`](https://github.com/sps014/dream/blob/main/sample/interop/http.dream).
