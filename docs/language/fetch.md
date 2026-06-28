# Fetch

`Fetch` is a small JS-like HTTP client built on Dream's [`extern async fun`](async.md#awaiting-javascript-promises) Promise bridge. The host (`runtime/dream.js`) forwards to the platform `fetch`, and the bridge turns the JavaScript `Promise` into a Dream `Future<T>` you can `await`.

!!! note "Runs under the JS runtime only"
    Like regex, `Fetch` depends on the `Dream` host module, so it works in the browser and Node, not the `wasmtime` harness.

## Fetching text

`Fetch.text(url)` returns the body directly:

```ts
async fun main(): void {
    let body = await Fetch.text("https://api.example.com/users/42");
    let user = JSON.parse(body);
    println(user.get("name").as_string());
}
```

## Richer responses

`Fetch.get(url)` resolves to a `Response` wrapping the live JS response as a [`JsRef`](references.md), so status and body are real:

```ts
async fun main(): void {
    let res = await Fetch.get("https://api.example.com/data");
    let status = res.status();      // 200
    let data = await res.json();    // JsonValue
    println(to_string(status));
    res.release();
}
```

## HTTP methods

Every verb is available. `get`/`delete`/`head` take just a URL; `post`/`put`/`patch` also take a request body; and `request` gives full control including custom headers (passed as a JSON-object string):

```ts
async fun main(): void {
    let created = await Fetch.post("https://api.example.com/users", "{\"name\":\"Grace\"}");
    println(to_string(created.status()));

    // Full control: method, url, body, and a JSON string of headers.
    let res = await Fetch.request("PUT", "https://api.example.com/users/1",
                                  "{\"name\":\"Ada\"}",
                                  "{\"Content-Type\":\"application/json\"}");
    let updated = await res.json();
}
```

| Member | Description |
| --- | --- |
| `Fetch.text(url): Future<string>` | GET and return the body as text |
| `Fetch.get(url): Future<Response>` | GET and return a `Response` |
| `Fetch.post/put/patch(url, body): Future<Response>` | send `body` with the given verb |
| `Fetch.delete/head(url): Future<Response>` | DELETE / HEAD request |
| `Fetch.request(method, url, body, headers): Future<Response>` | arbitrary verb; `headers` is a JSON-object string ("" for none) |
| `Response.status(): int` | HTTP status code |
| `Response.ok(): bool` | true for a 2xx status |
| `Response.text(): Future<string>` | body as text (async) |
| `Response.json(): Future<JsonValue>` | body parsed as [JSON](json.md) (async) |
| `Response.release(): void` | drop the underlying JS handle |

Because `Fetch` composes with [async methods](async.md#async-methods), you can `await Fetch.text(...)` inside your own `async fun` — including a class method. A runnable example lives in [`sample/interop/fetch.dream`](https://github.com/sps014/Dream/blob/main/sample/interop/fetch.dream) with its Node runner `fetch.mjs`.
