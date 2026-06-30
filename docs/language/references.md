# JavaScript References (`JsRef`)

`JsRef` is an opaque handle to a live JavaScript value held by the runtime. A real JS object (a DOM
node, a `Response`, a `RegExp`, a function) can cross into Dream as a `JsRef` and be read or called
through interop helpers, instead of being flattened to a string.

## How it works

A `JsRef` is an `i32` id into a host-side handle registry. When an interop function returns a JS
value typed `JsRef`, the runtime registers it and returns the id; when an id is passed back, the
runtime looks the value up. A `JsRef` is not a Dream heap object, so it is never reference-counted:
Dream will not free it for you.

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

```dream
@js("window", "document")
extern fun get_document(): JsRef;

fun main(): void {
    let doc = get_document();
    let title = doc.get_string("title");   // reads document.title
    println(title);
}
```

`JsRef.global(name)`, `JsRef.from_string(v)`, `JsRef.from_int(v)`, `JsRef.from_double(v)` and `JsRef.from_bool(v)` build references from Dream values or the global object.

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

```dream
let re = JsRef.global("RegExp");           // the RegExp constructor
// ... build/use a regex via call/get ...
re.release();
```

## Where it runs

`JsRef` relies on the `Dream` host module in `runtime/dream.js`, so it only works under the JS runtime (browser or Node), not the standalone `wasmtime` test harness. A runnable example lives in [`sample/interop/jsref.dream`](https://github.com/sps014/Dream/blob/main/sample/interop/jsref.dream) with its Node runner `jsref.mjs`.
