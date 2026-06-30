# Discriminated Unions & `match`

A plain [`enum`](types.md#enums) is a set of integer constants. When **any** variant carries a
payload `(...)`, the whole `enum` becomes a *discriminated union* (also called a tagged union or
algebraic data type): a value is exactly one of its variants, and each variant can hold its own
typed data. You take the data back out with an exhaustive `match`.

```dream
enum Shape {
    Circle(radius: float),
    Rect(width: float, height: float),
    Empty,                       // a unit variant carries no data
}
```

A union is heap-allocated and reference counted, just like a class, so it interoperates with
generics, `is`, and `to_string`.

## Constructing values

Construct a variant with member-access call syntax. Unit variants need no parentheses:

```dream
let s = Shape.Circle(2.0);
let r = Shape.Rect(3.0, 4.0);
let e = Shape.Empty;
```

## `match`

`match` inspects a union value and runs the first arm whose pattern fits. It works as both an
**expression** (every arm is `pattern => expr`, and all arms share one type) and a **statement**
(arms may be `=> { ... }` blocks run for their effects).

```dream
// expression position: yields a value
let area = match (s) {
    Circle(r)  => 3.14 * r * r,
    Rect(w, h) => w * h,
    Empty      => 0.0,
};

// statement position: arms may be blocks
match (s) {
    Circle(r)  => { println(r); }
    Rect(w, h) => println(w * h),
    Empty      => println("empty"),
}
```

The variant qualifier is optional inside `match` because the subject type is already known:
`Circle(r)` and `Shape.Circle(r)` are equivalent.

### Patterns

| Pattern | Matches |
|---------|---------|
| `_` | anything (the wildcard); binds nothing |
| `name` | anything; binds the value to `name` |
| `0`, `"hi"`, `true` | a value equal to the literal |
| `Variant(p1, p2, …)` | the given variant, matching each payload field against a sub-pattern |

Patterns nest, so a variant's fields can themselves be matched:

```dream
match (pair) {
    Both(Some(x), None) => x,
    _                   => 0,
}
```

### Guards

An arm may add an `if <bool>` guard. A guarded arm matches only when its pattern fits **and** the
guard is true:

```dream
match (opt) {
    Some(n) if n > 10 => println("big"),
    Some(n)           => println(n),
    None              => println("none"),
}
```

## Exhaustiveness

`match` must cover every case. The compiler rejects a `match` that omits a variant unless a `_`
(or a bare binding) catches the rest:

```dream
// error: missing variant(s) Empty
let area = match (s) {
    Circle(r)  => 3.14 * r * r,
    Rect(w, h) => w * h,
};
```

Because guards and literal sub-patterns can fail at runtime, they never count toward
exhaustiveness — an arm like `Some(0)` or `Some(n) if …` always needs a following catch-all
(`Some(n)`, a binding, or `_`). Likewise, nested variant patterns do not enumerate their inner
variants for you: a `Both(Some(x), None)` arm does not prove every `Both(…)` is handled, so add a
trailing binding/`_` arm. The compiler also reports **unreachable** arms that sit after a
catch-all.

## Generics

Unions may be generic; the concrete type is inferred from the constructor arguments, or supplied by
an annotation when it cannot be inferred (e.g. a unit variant):

```dream
enum Option<T> { Some(value: T), None }
enum Result<T, E> { Ok(value: T), Err(error: E) }

let o  = Option.Some(42);            // inferred Option<int>
let n: Option<int> = Option.None;    // annotation needed for the unit variant

fun safe_div(a: int, b: int): Result<int, string> {
    if (b == 0) {
        return Result.Err("divide by zero");
    }
    return Result.Ok(a / b);
}
```

## Methods on generic unions

A union can carry methods through an [`extend`](classes.md) block, and the block may be generic so
the methods are available on every instantiation. The type parameters declared on `extend` are
bound per instantiation, so `this` and the method signature refer to the concrete payload type:

```dream
enum Option<T> { Some(value: T), None }

extend Option<T> {
    public fun unwrap_or(fallback: T): T {
        return match (this) {
            Some(v) => v,
            None    => fallback,
        };
    }
    public fun is_some(): bool {
        return match (this) { Some(v) => true, None => false };
    }
}

fun main(): void {
    let o = Option.Some(7);
    println(o.unwrap_or(0));   // 7
    println(o.is_some());      // true
}
```

The prelude uses exactly this mechanism to give [`Option<T>`](../stdlib/option.md) and
[`Result<T, E>`](../stdlib/result.md) their `unwrap_or` / `is_some` / `is_ok` helpers, so you rarely
need to write the `match` by hand.

## Built-in `Option<T>` and `Result<T, E>`

These two unions are common enough that the standard library defines them for you — they are
auto-imported into every program, so you can use `Option.Some`/`Option.None` and
`Result.Ok`/`Result.Err` without declaring anything:

```dream
enum Option<T> { Some(value: T), None }          // provided by the prelude
enum Result<T, E> { Ok(value: T), Err(error: E) } // provided by the prelude
```

Use `Option<T>` for a value that may be absent and `Result<T, E>` for an operation that may fail.
Because they are ordinary discriminated unions, you read them back with `match` exactly as above.
(Do not redeclare them in your own program — that is a duplicate-definition error.)

## `to_string`, `hash_code`, and `object`

Unions take part in the [object protocol](../stdlib/builtins.md) like every other type. The default
`to_string` is variant-aware: a data variant renders as `Variant(field: value, ...)` and a unit
variant as just its name. `hash_code` mixes in the active variant's discriminant and payload, so
different variants (and different payloads) hash differently.

```dream
enum Shape { Circle(radius: int), Rect(width: int, height: int), Empty }

let c = Shape.Circle(5);
println(c.to_string());            // Circle(radius: 5)
println(Shape.Empty.to_string());  // Empty

let o: object = c;                 // a union is already a tagged heap value
println(o.to_string());            // Circle(radius: 5)
```

Provide an `@override public fun to_string(): string` (or `hash_code`) in an `extend` block to
replace the default.

## JSON with `@json`

Mark a discriminated union `@json` to derive [`to_json` / `from_json`](../stdlib/json.md#unions)
converters. Each value serializes to an object tagged with a `"type"` key naming the active variant:

```dream
@json
enum Shape { Circle(radius: int), Rect(width: int, height: int), Empty }

let text = JSON.serialize(Shape.Circle(7));   // {"type":"Circle","radius":7}
let back = JSON.deserialize<Shape>(text);      // Shape.Circle(7)
```

## When to use `match` vs `switch`

Use [`switch`](control-flow.md#switch) for plain C-style enums and integer/string values. Use
`match` for discriminated unions: it destructures payloads and is checked for exhaustiveness, which
`switch` is not.
