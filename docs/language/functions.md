# Functions

## Defining a function

```kotlin
fun add(a: int, b: int): int {
    return a + b;
}
```

- `fun` keyword, then the name.
- Parameters are `name: type` pairs separated by commas.
- `: ReturnType` after the parameter list.

The return type is optional for functions that return nothing. These two are equivalent:

```kotlin
fun greet() {
    println("hi");
}

fun greet(): void {
    println("hi");
}
```

## Calling a function

```kotlin
let result = add(3, 4);
```

## Returning a value

Use `return`:

```kotlin
fun clamp(value: int, lo: int, hi: int): int {
    if (value < lo) { return lo; }
    if (value > hi) { return hi; }
    return value;
}
```

The compiler checks that all code paths return a value when the return type is not `void`.

## Recursion

Functions can call themselves:

```kotlin
fun fib(n: int): int {
    if (n <= 1) { return n; }
    return fib(n - 1) + fib(n - 2);
}
```

## Generic functions

Add `<TypeParam>` after the function name to make it generic. The type parameter stands in for any concrete type:

```kotlin
fun identity<T>(value: T): T {
    return value;
}

fun main() {
    println(identity<int>(42));
    println(identity<string>("hello"));
}
```

The compiler generates a separate copy of the function body for each distinct type you use it with. There is no runtime overhead from generics.

Multiple type parameters are allowed:

```kotlin
fun pair_first<A, B>(a: A, b: B): A {
    return a;
}
```

## First-class functions

A function name used as a value refers to the function itself, and the function type is written `fun(ParamTypes): ReturnType`. Functions can be stored in variables and passed as parameters, then invoked like any other call:

```kotlin
fun twice(x: int): int {
    return x * 2;
}

fun apply(f: fun(int): int, value: int): int {
    return f(value);
}

fun main() {
    let g: fun(int): int = twice;
    println(g(5));            // 10
    println(apply(twice, 8)); // 16
}
```

Closures (capturing surrounding variables) are not yet supported.

## Exported functions

Mark a function `pub` to expose it to the WebAssembly host environment:

```kotlin
pub fun compute(n: int): int {
    return n * n;
}
```

Exported functions cannot expose structs that are not themselves exported.

## Entry point

The runtime calls `main` to start the program. Every runnable MinLang program needs one. The return type can be omitted:

```kotlin
fun main() {
    println("hello");
}
```

## Imports

Split a program across files with `import`. See [Imports](imports.md).
