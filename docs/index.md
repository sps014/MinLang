# Dream

Dream is a statically typed language that compiles to WebAssembly. It has a simple, readable syntax and manages memory automatically — no garbage collector, no manual frees.

```ts
fun greet(name: string): string {
    return "Hello, " + name;
}

fun main() {
    println(greet("world"));
}
```

## What it is

- **Statically typed** — every variable and expression has a type, checked at compile time.
- **Compiles to WASM** — the output is a `.wat` file you can run with any WebAssembly runtime.
- **Automatic memory management** — reference counting keeps allocations clean without a GC pause.
- **Generics** — write one function or class, get specialized code for every type you use it with.
- **Standard collections** — `List<T>` and `Map<K, V>` are built in, no imports needed.

## Start here

New to Dream? Follow the [Getting Started](getting-started.md) guide to install the compiler, write your first program, and run it.

If you already know the basics, the [Language](language/variables.md) section covers everything in detail.

## Standard library

| Page | Description |
|------|-------------|
| [Built-ins](stdlib/builtins.md) | `print`, `println`, `to_string`, `hash_code`, `array_new`, `Math.*` |
| [string](stdlib/string.md) | String methods: `substring`, `contains`, `trim`, `to_lower`, … |
| [int](stdlib/int.md) | Integer methods: `abs`, `min`, `max`, `clamp`, `pow`, `signum`; static `int.parse` |
| [float](stdlib/float.md) | Float methods: `abs`, `min`, `max` |
| [double](stdlib/double.md) | Double methods: `abs`, `min`, `max` |
| [char](stdlib/char.md) | Character methods: `is_digit`, `is_alpha`, `to_lower`, `to_upper`, `as_string`, … |
| [bool](stdlib/bool.md) | Boolean methods: `to_int` |
| [List\<T\>](stdlib/list.md) | Growable sequence: `push`, `pop`, `get`, `set`, `remove_at`, … |
| [Map\<K, V\>](stdlib/map.md) | Hash map: `put`, `get`, `contains`, `remove`, `keys`, `values`, … |

