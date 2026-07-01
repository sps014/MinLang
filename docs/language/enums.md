# Enums

A C-style enum defines a set of named integer constants. Members are numbered from `0` by default;
an explicit value makes the following members continue from it:

```dream
enum Color { Red, Green, Blue }          // 0, 1, 2
enum Status { Active = 10, Inactive }    // 10, 11
```

Access a member with `Enum.Member`. Enum values are integers at runtime, so they interoperate with
`int` and work as [`switch`](control-flow.md#switch-over-enums) subjects and labels:

```dream
let c: Color = Color.Green;
println(c);              // 1
```

Call `.name()` on an enum value to get its member name as a string:

```dream
println(Color.Green.name());   // Green
println(c.name());             // Green
```

## Enums with data

When a variant carries a typed payload `(...)`, the `enum` becomes a heap-backed *discriminated
union*: each variant can hold its own data, and you read it back with the pattern-matching form of
`switch` (`pattern => body`) instead of the C-style `case`/`default` form.

```dream
enum Shape {
    Circle(radius: float),
    Rect(width: float, height: float),
    Empty,
}
```

See [Discriminated Unions](discriminated-unions.md) for the full syntax, pattern matching, and
generics.
