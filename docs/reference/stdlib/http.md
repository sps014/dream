# HTTP

**Import:** `import system.net;`

```dream
import system;
import system.net;

async fun main(): void {
    let api = HttpClient("https://api.example.com");
    switch (await api.get("/health")) {
        Ok(res) => System.println(res.status()),
        Err(e) => System.println(e.message()),
    }
}
```

Native uses the host HTTP client (reqwest in `libdream`); browser and Node use `fetch`. Buffered `get`/`post` and streaming `get_stream`/`request_stream` share that path. Calls are `async` and return `Result`. A convenient public mock API for experiments is [JSONPlaceholder](https://jsonplaceholder.typicode.com/guide/).

## Client

| Call | Meaning |
| --- | --- |
| `HttpClient(base_url)` | `""` if every URL is absolute |
| `set_header(name, value)` | default header (chains) |
| `with_timeout(ms)` | `0` = none |
| `with_http_version(1 or 2)` | native only |
| `with_cookie_jar(jar)` | cookies |
| `with_cancellation(token)` | cooperative cancel |
| `await text(path)` | GET, body as string |
| `await get` / `post` / `put` / `patch` / `delete` / `head` | full response |
| `await request(method, path, body, headers)` | custom |
| `await post_bytes` / `put_bytes` / `request_bytes` | binary body |
| `await post_multipart(path, form)` | multipart |
| `await get_stream` / `request_stream` | chunked body |

## Response and helpers

`HttpResponse`: `status()`, `ok()`, `header(name)`, `text()`, `bytes()`, `json()`.

`HttpHeaders`: `set` / `get` / `contains` / `remove` / `.length` / `to_wire` / `from_wire`.

`CookieJar`: `set` / `get` / `clear` / `to_header` / `store_from_response`.

`HttpStreamResponse`: `read_chunk`, `read_all`, `close`.

`MultipartForm`: `add_field` / `add_file` / `build()`.

`Url.parse`, `to_string()`, `with_path`, `join`.

Pair with [JSON](json.md). Example: [`sample/interop/http.dream`](https://github.com/sps014/dream/blob/main/sample/interop/http.dream).
