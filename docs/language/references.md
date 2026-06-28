# JavaScript References (`JsRef`)

`JsRef` is an opaque handle to a live JavaScript value held by the runtime (`runtime/dream.js`). It lets a real JS object — a DOM node, a `Response`, a `RegExp`, a function — cross into Dream and be read or called through generic interop helpers, instead of being flattened to a string.

## How it works

At runtime a `JsRef` is just an `i32` id into a host-side **handle registry**. When an interop function returns a JS value typed `JsRef`, the runtime registers it and returns its id; when an id is passed back in, the runtime looks the value up. Because a `JsRef` is *not* a Dream heap object, it is **never reference-counted** — Dream will not free it for you.

```mermaid
flowchart LR
  js["JS value (RegExp / Response / fn)"] -->|registerHandle| id["i32 id"]
  id -->|"JsRef in Dream"| dream["Dream code"]
  dream -->|"id passed back"| deref["derefHandle -> JS value"]
```

!!! warning "Release long-lived handles"
    Call `.release()` when you are done with a long-lived handle to drop the host-side entry and avoid leaking it.

## Getting a reference

The prelude provides a few constructors plus access to the JS global scope:

```ts
@js("window", "document")
extern fun get_document(): JsRef;

fun main(): void {
    let doc = get_document();
    let title = doc.get_string("title");   // reads document.title
    println(title);
}
```

`js_global(name)`, `js_string(v)`, `js_int(v)`, `js_double(v)` and `js_bool(v)` build references from Dream values or the global object.

## The `JsRef` API

| Method | Description |
| --- | --- |
| `get(name): JsRef` | read property `name` as another reference |
| `get_string/get_int/get_double/get_bool(name)` | read a property coerced to a primitive |
| `set(name, value: JsRef): void` | set a property |
| `call(name): JsRef` / `call1` / `call2` | invoke method `name` with 0/1/2 reference args |
| `invoke(): JsRef` / `invoke1` / `invoke2` | call the reference itself as a function (see [Callbacks](callbacks.md)) |
| `text(): string` | the JS `String(value)` of the referenced value |
| `is_null(): bool` | true if the value is `null` or `undefined` |
| `release(): void` | drop the host-side handle |

```ts
let re = js_global("RegExp");           // the RegExp constructor
// ... build/use a regex via call/get ...
re.release();
```

## Where it runs

`JsRef` relies on the `Dream` host module in `runtime/dream.js`, so it only works under the JS runtime (browser or Node), not the standalone `wasmtime` test harness. A runnable example lives in [`sample/interop/jsref.dream`](https://github.com/sps014/Dream/blob/main/sample/interop/jsref.dream) with its Node runner `jsref.mjs`.
