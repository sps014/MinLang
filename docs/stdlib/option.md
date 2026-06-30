# `Option<T>`

`Option<T>` represents a value that may be absent. It is a [discriminated
union](../language/discriminated-unions.md) with two variants:

```dream
enum Option<T> { Some(value: T), None }   // provided by the prelude
```

It is auto-imported into every program. Use it instead of a nullable type when the absence is part
of the value's meaning — for example a lookup that may find nothing, or a parse that may not
produce a result.

## Constructing

```dream
let some = Option.Some(42);          // inferred Option<int>
let none: Option<int> = Option.None; // annotation needed for the unit variant
```

`Some` carries a value of type `T`. `None` carries nothing, so its element type cannot be inferred
on its own; annotate the binding (or rely on the surrounding context, such as a function return
type) when constructing a bare `None`.

## Reading the value

Destructure an `Option<T>` with [`match`](../language/discriminated-unions.md). The match is
checked for exhaustiveness, so both variants must be handled:

```dream
fun unwrap_or(o: Option<int>, fallback: int): int {
    return match (o) {
        Some(v) => v,
        None    => fallback,
    };
}

fun main(): void {
    println(unwrap_or(Option.Some(7), 0));   // 7
    println(unwrap_or(Option.None, 0));      // 0
}
```

A guard narrows an arm further:

```dream
match (o) {
    Some(n) if n > 100 => println("large"),
    Some(n)            => println(n),
    None               => println("absent"),
}
```

## Helper methods

For the common cases the prelude provides methods on `Option<T>` so you do not have to write a full
`match` every time:

| Method | Returns |
| --- | --- |
| `is_some(): bool` | `true` when this is `Some` |
| `is_none(): bool` | `true` when this is `None` |
| `unwrap_or(fallback: T): T` | the contained value, or `fallback` when `None` |

```dream
let o = Option.Some(7);
println(o.unwrap_or(0));   // 7
println(o.is_some());      // true

let n: Option<int> = Option.None;
println(n.unwrap_or(0));   // 0
```

These are defined with a generic `extend Option<T> { ... }` block (see [Discriminated
unions](../language/discriminated-unions.md#methods-on-generic-unions)). There is deliberately no
panicking `unwrap()` — use `unwrap_or` or `match` so the empty case is always handled.

## `Option<T>` vs `T?`

A nullable type (`T?`) and `Option<T>` both model absence. Prefer `Option<T>` when you want the
compiler to force every reader to handle the empty case through `match`; prefer `T?` for the
lightweight `null` checks and `??` fallback described in [Types](../language/types.md#nullable-types).
