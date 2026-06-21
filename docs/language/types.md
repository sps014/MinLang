# Types

## Primitives

| Type     | Description                        | Example literal  |
|----------|------------------------------------|-----------------|
| `int`    | 32-bit signed integer              | `42`, `-7`       |
| `float`  | 32-bit floating point              | `3.14f`, `1.0`   |
| `double` | 64-bit floating point              | `3.14d`, `1.0d`  |
| `bool`   | Boolean (`true` or `false`)        | `true`           |
| `string` | UTF-8 text, heap allocated         | `"hello"`        |
| `void`   | No value — only valid as a return type | —            |

## Arrays

Append `[]` to any type to get an array of that type:

```kotlin
let nums: int[] = [10, 20, 30];
let names: string[] = ["a", "b", "c"];
```

Array access is zero-indexed:

```kotlin
let first = nums[0];   // 10
nums[1] = 99;
```

Arrays are fixed-size once created from a literal. For a growable list, use [`List<T>`](../stdlib/collections.md).

## Nullable types

Any reference type can be marked nullable with `?`. A nullable variable may hold either a real value or `null`:

```kotlin
let node: Node? = null;
node = Node { value: 5, next: null };
```

Primitive types (`int`, `float`, `double`, `bool`) cannot be nullable.

The null-coalescing operator `??` provides a fallback for nullable values (see [operators](operators.md)).

## Char literals

A character literal in single quotes is an `int` holding the character's code point. Common escapes (`'\n'`, `'\t'`, `'\r'`, `'\0'`, `'\\'`, `'\''`) are supported:

```kotlin
let a: int = 'A';        // 65
let newline: int = '\n'; // 10
let next: int = 'A' + 1; // 66
```

## Enums

A C-style enum defines a set of named integer constants. Members are numbered from `0` by default, and an explicit value makes subsequent members continue from it:

```kotlin
enum Color { Red, Green, Blue }          // 0, 1, 2
enum Status { Active = 10, Inactive }    // 10, 11
```

Access a member with `Enum.Member`. Enum values are integers at runtime, so they interoperate with `int` (e.g. they can be passed to `print_int`) and work as [`switch`](control-flow.md#switch-over-enums) subjects and labels:

```kotlin
let c: Color = Color.Green;
print_int(c);            // 1
```

## Type aliases

`type` introduces an alias for an existing type. Aliases are resolved at compile time (they are interchangeable with the underlying type) and must be declared before use:

```kotlin
type Number = int;
type Names = string[];

fun add(a: Number, b: Number): Number {
    return a + b;
}
```

## Structs

User-defined types. See [Structs](structs.md).

## The `object` type

A universal container that can hold any value at runtime. Useful for heterogeneous collections and runtime type dispatch. See [The object type](objects.md).

## Type casting

Use a C-style cast to convert between numeric types or between a value and `object`:

```kotlin
let n = 7;
let f = (float)n;        // int -> float
let back = (int)f;       // float -> int

let o: object = n;       // boxing — int stored as object
let unboxed = (int)o;    // unboxing — traps if wrong type at runtime
```

Supported conversions: `int ↔ float`, `int ↔ double`, `float ↔ double`, any type ↔ `object`.
