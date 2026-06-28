# Regex

Dream ships no regular-expression engine of its own. Instead the `Regex` class is a thin wrapper that exposes JavaScript's `RegExp` through [interop](interop.md), so the full ECMAScript pattern syntax and flags are available for free.

!!! note "Runs under the JS runtime only"
    Because `Regex` calls into the `Dream` host module in `runtime/dream.js`, it works in the browser and Node, not the standalone `wasmtime` harness.

## Usage

Construct a `Regex` with a pattern and flags, then test, replace, or match:

```ts
let re = Regex { pattern: "\\d+", flags: "g" };

if (re.test("abc123")) {
    println("has digits");
}

let cleaned = re.replace("a1b2c3", "#");   // a#b#c#
let parts = re.match("a1b2");              // ["1", "2"]
```

| Method | Description |
| --- | --- |
| `test(input): bool` | true if `input` contains a match |
| `replace(input, replacement): string` | replace matches (use the `g` flag for all; `$1` group refs supported) |
| `match(input): string[]` | the matches (every match with `g`, else the full match + capture groups) |
| `compile(): JsRef` | a live JS `RegExp` [handle](references.md) for stateful use (`exec`, `lastIndex`) |

## Stateful use via `JsRef`

When you need the stateful `RegExp` API, `compile()` hands back the real object as a [`JsRef`](references.md):

```ts
let re = Regex { pattern: "\\w+", flags: "g" };
let rx = re.compile();      // a live JS RegExp
// ... rx.call1("exec", js_string(input)), rx.get_int("lastIndex"), ...
rx.release();               // drop the handle when done
```

A runnable example lives in [`sample/interop/regex.dream`](https://github.com/sps014/Dream/blob/main/sample/interop/regex.dream) with its Node runner `regex.mjs`.
