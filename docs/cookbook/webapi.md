# JSON HTTP API

A tiny native server with `@get` / `@post`, JSON bodies, and Swagger at `/docs`.

```dream
import system;
import system.json;
import system.webapi;

@json
class Item {
    public id: int;
    public name: string;
    public constructor(id: int, name: string) {
        this.id = id;
        this.name = name;
    }
}

@get("/items/{id}")
fun get_item(id: int): Item {
    return Item(id, "demo");
}

async fun main(): void {
    WebApp.use(CORS(CorsOptions()));
    await WebApp.run("127.0.0.1", 8080);
}
```

```bash
dream run sample/webapi/app.dream
```

Open [http://127.0.0.1:8080/docs](http://127.0.0.1:8080/docs). Full reference: [system.webapi](../reference/stdlib/webapi.md).
