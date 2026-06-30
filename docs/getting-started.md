# Getting Started

## Prerequisites

You need [Rust](https://rustup.rs) installed. That's it.

## Install

```bash
git clone https://github.com/sps014/MinLang
cd Dream
cargo build --release
```

The binary ends up at `target/release/dream`. You can run it directly from that path, or just use `cargo run --` as shown throughout this guide.

## Your first program

Create a file called `hello.dream`:

```dream
fun main() {
    println("Hello, world!");
}
```

Run it:

```bash
cargo run -- run hello.dream
```

Output:

```
Hello, world!
```

That's it. The `run` subcommand compiles your file and executes it immediately using Wasmtime.

If you want to inspect the generated WebAssembly, drop the `run` subcommand:

```bash
cargo run -- hello.dream
```

This writes a `hello.wat` file next to your source.

## A slightly bigger example

```dream
fun factorial(n: int): int {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

fun main() {
    let i = 1;
    while (i <= 10) {
        println(factorial(i));
        i = i + 1;
    }
}
```

Things to notice:

- `fun` declares a function. The return type comes after `:`.
- `let` declares a local variable. The type is inferred from the initializer.
- `print` writes a value with no newline; `println` appends one. Both work on any type — int, float, string, bool, char, classes.
- The return type is optional when a function returns nothing (`fun main()`).
- Conditions need parentheses: `if (n <= 1)`, not `if n <= 1`.

## Next steps

- [Variables](language/variables.md) — declaration, inference, and assignment rules.
- [Types](language/types.md) — the full type system including nullable and arrays.
- [Classes](language/classes.md) — define your own data types with methods.
- [List](stdlib/list.md) and [Map](stdlib/map.md) — the standard collections.
