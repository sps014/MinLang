# `Result<T, E>`

`Result<T, E>` represents the outcome of an operation that can fail: either a success value (`Ok`)
or an error (`Err`). It is a [discriminated
union](../language/discriminated-unions.md) with two variants:

```dream
enum Result<T, E> { Ok(value: T), Err(error: E) }   // provided by the prelude
```

It is auto-imported into every program. Returning a `Result` makes failure part of a function's
type, so callers cannot ignore it the way they might ignore a sentinel return value.

## Returning a result

```dream
fun safe_div(a: int, b: int): Result<int, string> {
    if (b == 0) {
        return Result.Err("divide by zero");
    }
    return Result.Ok(a / b);
}
```

`Ok` carries the success value of type `T`; `Err` carries the error of type `E`. The error type is
arbitrary — a `string` message, an error code, or your own class.

## Handling a result

Destructure a `Result<T, E>` with [`match`](../language/discriminated-unions.md). Both variants
must be handled:

```dream
fun main(): void {
    match (safe_div(10, 2)) {
        Ok(v)  => println(v),    // 5
        Err(e) => println(e),
    }

    match (safe_div(1, 0)) {
        Ok(v)  => println(v),
        Err(e) => println(e),    // divide by zero
    }
}
```

## `Result<T, E>` vs `Option<T>`

Use [`Option<T>`](option.md) when a value is simply present or absent. Use `Result<T, E>` when the
absence has a reason you want to carry along — the `Err` payload explains *why* the operation did
not produce a value.
