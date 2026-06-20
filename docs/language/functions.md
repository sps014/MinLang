# Functions

## Defining a function

```minlang
fun add(a: int, b: int): int {
    return a + b;
}
```

- `fun` keyword, then the name.
- Parameters are `name: type` pairs separated by commas.
- `: ReturnType` after the parameter list. Use `void` when the function returns nothing.

## Calling a function

```minlang
let result = add(3, 4);
```

## Returning a value

Use `return`:

```minlang
fun clamp(value: int, lo: int, hi: int): int {
    if (value < lo) { return lo; }
    if (value > hi) { return hi; }
    return value;
}
```

The compiler checks that all code paths return a value when the return type is not `void`.

## Recursion

Functions can call themselves:

```minlang
fun fib(n: int): int {
    if (n <= 1) { return n; }
    return fib(n - 1) + fib(n - 2);
}
```

## Generic functions

Add `<TypeParam>` after the function name to make it generic. The type parameter stands in for any concrete type:

```minlang
fun identity<T>(value: T): T {
    return value;
}

fun main(): void {
    print(identity<int>(42));
    print(identity<string>("hello"));
    print("\n");
}
```

The compiler generates a separate copy of the function body for each distinct type you use it with. There is no runtime overhead from generics.

Multiple type parameters are allowed:

```minlang
fun pair_first<A, B>(a: A, b: B): A {
    return a;
}
```

## Exported functions

Mark a function `export` to expose it to the WebAssembly host environment:

```minlang
export fun compute(n: int): int {
    return n * n;
}
```

Exported functions cannot expose structs that are not themselves exported.

## Entry point

The runtime calls `fun main(): void` to start the program. Every runnable MinLang program needs one.
