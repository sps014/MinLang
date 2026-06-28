# JSON

Dream ships a native JSON library in the prelude — a hand-written parser and stringifier plus a `JsonValue` model — and a compiler-supported `@json` auto-derive for your own classes. None of it requires interop, so it runs everywhere Dream does (including the `wasmtime` test harness).

## The `JsonValue` model

Since Dream has no tagged unions, JSON is modeled with a kind-tagged `JsonValue` class. Build values with the static constructors and read them with typed accessors:

```ts
let user = JsonValue.dict();
user.set("name", JsonValue.from_string("Ada"));
user.set("age", JsonValue.from_int(36));

let tags = JsonValue.array();
tags.push(JsonValue.from_string("dev"));
user.set("tags", tags);
```

| Constructor | Builds |
| --- | --- |
| `JsonValue.none()` | `null` |
| `JsonValue.boolean(b)` | a boolean |
| `JsonValue.number(d)` / `JsonValue.from_int(n)` | a number |
| `JsonValue.from_string(s)` | a string |
| `JsonValue.array()` | an empty array |
| `JsonValue.dict()` | an empty object |

| Accessor | Returns |
| --- | --- |
| `as_bool() / as_int() / as_double() / as_string()` | the scalar value |
| `get(key): JsonValue` | object member by key |
| `at(index): JsonValue` | array element by index |
| `set(key, v) / push(v)` | mutate an object / array |
| `size(): int` | array length |

## `JSON.parse` / `JSON.stringify`

The `JSON` static class is the public entry point for the value model:

```ts
let text = JSON.stringify(user);     // {"name":"Ada","age":36,"tags":["dev"]}

let v = JSON.parse(text);
println(v.get("name").as_string());          // Ada
println(to_string(v.get("age").as_int()));   // 36
```

`JSON.parse` is a recursive-descent parser; `JSON.stringify` walks the value and escapes strings.

## Auto-derive with `@json`

Marking a class `@json` makes the compiler generate `to_json` / `from_json` converters for it, so the class round-trips with no boilerplate. Fields may be primitives, `string`, other `@json` classes, and arrays of those.

```ts
@json
class Address { city: string; zip: string; }

@json
class User { name: string; age: int; address: Address; tags: string[]; }

fun main(): void {
    let u = User {
        name: "Ada", age: 36,
        address: Address { city: "London", zip: "NW1" },
        tags: ["dev", "math"],
    };

    let text = JSON.serialize(u);              // compiler-generated to_json + stringify
    let back = JSON.deserialize<User>(text);   // parse + compiler-generated from_json
    println(back.address.city);                // London
}
```

- `JSON.serialize(x): string` — stringifies any `@json` value.
- `JSON.deserialize<T>(text): T` — parses `text` and reconstructs a `T`.

!!! note "v1 limits"
    `@json` classes must be non-generic, and their fields are limited to primitives, `string`, arrays of those, and other `@json` classes. Calling `JSON.serialize`/`deserialize` on a type without a derived converter is a compile-time error.
