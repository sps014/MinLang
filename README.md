# MinLang Compiler

MinLang is a statically typed, compiled programming language that targets WebAssembly (WASM). It features a clean syntax, scope-based memory management, and a robust compiler pipeline written entirely in Rust.

## Features

- **Static Typing**: Supports `int`, `float`, `string`, `bool`, and `void` types.
- **Type Casting**: C-style explicit type casting (e.g., `(float)10`, `(int)3.14`).
- **Structs**: User-defined composite data types with field access and assignment.
- **Arrays**: Native support for arrays (`int[]`, `float[]`, `string[]`, `Struct[]`).
- **Memory Management**: Automatic Reference Counting (ARC) backed by a fast Freelist allocator in WebAssembly.
- **Control Flow**: `if`/`else if`/`else`, `while` loops, and `for` loops with `break` and `continue` support.
- **Functions**: First-class functions with parameters and return types.
- **WebAssembly Target**: Compiles directly to WebAssembly Text format (`.wat`) and executes via `wasmtime`.
- **Standard Library**: Built-in functions like `print`, `println`, `print_int`, `print_float`, `sin`, `cos`, `abs`, and `sqrt`.
- **Diagnostic System**: Comprehensive error reporting with line/column tracking for syntax and semantic errors.

## Architecture

The MinLang compiler pipeline consists of four main stages:

1. **Lexer**: Tokenizes the source code into a stream of syntax tokens.
2. **Parser**: Constructs an Abstract Syntax Tree (AST) using a recursive descent parsing strategy.
3. **Semantic Analyzer**: Performs type checking, scope resolution, and control flow analysis to ensure program correctness.
4. **Code Generator**: Translates the validated AST into WebAssembly Text format (`.wat`), handling memory allocation, function calls, and control structures.

## Prerequisites

To build and run the MinLang compiler, you need:

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)
- Cargo (included with Rust)

## Installation

Clone the repository and build the project using Cargo:

```bash
git clone <repository-url>
cd MinLang
cargo build --release
```

## Usage

You can use the MinLang compiler via its command-line interface.

### Running a Program

To compile and immediately execute a MinLang file:

```bash
cargo run -- run path/to/your/file.ml
```

### Compiling a Program

To compile a MinLang file to WebAssembly Text format (`.wat`) without executing it:

```bash
cargo run -- compile path/to/your/file.ml
```

## Language Examples

### Hello World

```minlang
fun main(): void {
    println("Hello, World!");
}
```

### Arithmetic & Control Flow

```minlang
fun fib(n: int): int {
    if n <= 1 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fun main(): void {
    let result = fib(10);
    print("Fibonacci of 10 is: ");
    print_int(result);
}
```

### Structs & Memory Management

```minlang
struct Point {
    x: int;
    y: int;
}

struct Rect {
    p1: Point;
    p2: Point;
}

fun main(): void {
    let p1 = Point { x: 10, y: 20 };
    let p2 = Point { x: 30, y: 40 };
    
    let r = Rect { p1: p1, p2: p2 };
    
    print_int(r.p1.x); // 10
    
    // Memory is allocated via a fast Freelist allocator
    // and managed via Automatic Reference Counting (ARC)
}
```

### Arrays & Loops

```minlang
fun main(): void {
    let arr: int[] = [10, 20, 30, 40, 50];
    let sum = 0;
    
    for let i = 0; i < 5; i = i + 1 {
        sum = sum + arr[i];
    }
    
    print("Sum of array elements: ");
    print_int(sum);
}
```

## Testing

MinLang includes a comprehensive test suite covering unit tests for compiler stages and end-to-end (E2E) tests that execute compiled WebAssembly directly in Rust using `wasmtime`.

To run the test suite:

```bash
cargo test
```

Test cases are located in `tests/cases/` and include `.ml` source files alongside their `.expected` outputs.

## License

This project is licensed under the MIT License.
