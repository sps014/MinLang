# MinLang

MinLang is a statically typed language that compiles to WebAssembly. It has a simple, readable syntax and manages memory automatically — no garbage collector, no manual frees.

```kotlin
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
- **Generics** — write one function or struct, get specialized code for every type you use it with.
- **Standard collections** — `List<T>` and `Map<K, V>` are built in, no imports needed.

## Start here

New to MinLang? Follow the [Getting Started](getting-started.md) guide to install the compiler, write your first program, and run it.

If you already know the basics, the [Language](language/variables.md) section covers everything in detail.
