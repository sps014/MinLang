# Types

## Primitives

| Type     | Description                        | Example literal  |
|----------|------------------------------------|-----------------|
| `int`    | 32-bit signed integer              | `42`, `-7`       |
| `float`  | 32-bit floating point              | `3.14f`, `1.0`   |
| `double` | 64-bit floating point              | `3.14d`, `1.0d`  |
| `bool`   | Boolean (`true` or `false`)        | `true`           |
| `char`   | A single character (code point)    | `'A'`, `'\n'`    |
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

Arrays are fixed-size once created from a literal. For a growable list, use [`List<T>`](../stdlib/list.md).

## Nullable types

Any reference type can be marked nullable with `?`. A nullable variable may hold either a real value or `null`:

```kotlin
let node: Node? = null;
node = Node { value: 5, next: null };
```

Primitive types (`int`, `float`, `double`, `bool`, `char`) cannot be nullable.

The null-coalescing operator `??` provides a fallback for nullable values (see [operators](operators.md)).

## char

`char` is a dedicated single-character type. A character literal is written in single quotes, and common escapes (`'\n'`, `'\t'`, `'\r'`, `'\0'`, `'\\'`, `'\''`) are supported. Each `char` occupies one byte in memory (in arrays and struct fields), making `char[]` a compact byte/character buffer:

```kotlin
let a: char = 'A';
let newline: char = '\n';
print(a);                  // prints "A"

let letters: char[] = ['h', 'i'];
print(letters[0]);         // prints "h"
```

A `char` and an `int` convert losslessly via a cast (a `char` is a code point):

```kotlin
let code: int = (int)a;       // 65
let next: char = (char)(code + 1);  // 'B'
```

## Enums

A C-style enum defines a set of named integer constants. Members are numbered from `0` by default, and an explicit value makes subsequent members continue from it:

```kotlin
enum Color { Red, Green, Blue }          // 0, 1, 2
enum Status { Active = 10, Inactive }    // 10, 11
```

Access a member with `Enum.Member`. Enum values are integers at runtime, so they interoperate with `int` and work as [`switch`](control-flow.md#switch-over-enums) subjects and labels:

```kotlin
let c: Color = Color.Green;
println(c);              // 1
```

Call `.name()` on an enum value to get its variant name as a string:

```kotlin
println(Color.Green.name());   // Green
println(c.name());             // Green
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

Supported conversions: `int ↔ float`, `int ↔ double`, `float ↔ double`, `int ↔ char`, any type ↔ `object`.
