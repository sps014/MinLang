# Getting Started

## Prerequisites

You need [Rust](https://rustup.rs) installed. That's it.

## Install

```bash
git clone https://github.com/sps014/MinLang
cd MinLang
cargo build --release
```

The binary ends up at `target/release/min_lang`. You can run it directly from that path, or just use `cargo run --` as shown throughout this guide.

## Your first program

Create a file called `hello.ml`:

```kotlin
fun main(): void {
    print("Hello, world!\n");
}
```

Run it:

```bash
cargo run -- run hello.ml
```

Output:

```
Hello, world!
```

That's it. The `run` subcommand compiles your file and executes it immediately using Wasmtime.

If you want to inspect the generated WebAssembly, drop the `run` subcommand:

```bash
cargo run -- hello.ml
```

This writes a `hello.wat` file next to your source.

## A slightly bigger example

```kotlin
fun factorial(n: int): int {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

fun main(): void {
    let i = 1;
    while (i <= 10) {
        print(factorial(i));
        i = i + 1;
    }
}
```

Things to notice:

- `fun` declares a function. The return type comes after `:`.
- `let` declares a local variable. The type is inferred from the initializer.
- `print` works on any type — int, float, string, bool, structs.
- Conditions need parentheses: `if (n <= 1)`, not `if n <= 1`.

## Next steps

- [Variables](language/variables.md) — declaration, inference, and assignment rules.
- [Types](language/types.md) — the full type system including nullable and arrays.
- [Structs](language/structs.md) — define your own data types with methods.
- [Collections](stdlib/collections.md) — `List<T>` and `Map<K, V>`.
