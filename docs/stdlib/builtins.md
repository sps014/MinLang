# Built-in Functions

These are available in every MinLang program without any import.

## print

Prints a value to stdout. Works on all types.

```kotlin
print(42);         // prints "42\n"
print(3.14f);      // prints "3.14\n"
print("hello");    // prints "hello"  (no automatic newline for strings)
print(true);       // prints "true"
```

!!! note "Newlines"
    `print` adds a trailing newline for numeric types (`int`, `float`, `double`) but not for `string` or `bool`. Use `print("\n")` to add one explicitly.

For structs that override `to_string`, `print` calls the override automatically.

## to_string

Converts any value to its string representation:

```kotlin
let s = to_string(42);      // "42"
let b = to_string(true);    // "true"
let f = to_string(3.14f);   // "3.14"
```

For structs with a `@override export fun to_string()` method, that method is called.

## hash_code

Returns a stable `int` hash for any value:

```kotlin
let h = hash_code("hello");
let h2 = hash_code(42);
```

Used internally by `Map<K, V>` to find buckets.

## Math

| Function | Signature             | Description          |
|----------|-----------------------|----------------------|
| `sin`    | `(float) -> float`    | Sine (radians)       |
| `cos`    | `(float) -> float`    | Cosine (radians)     |
| `sqrt`   | `(float) -> float`    | Square root          |
| `abs`    | `(float) -> float`    | Absolute value       |

```kotlin
let hyp = sqrt(3.0f * 3.0f + 4.0f * 4.0f);   // 5.0
```

## len

Returns the number of elements in an array:

```kotlin
let arr = [10, 20, 30];
print(len(arr));   // 3
```

## array_new

Allocates a zeroed array of a given size. Mainly used by the standard library internals. You can use it in your own code when you need an array whose size isn't known at compile time:

```kotlin
let buf = array_new<int>(100);   // int[] with 100 zero-initialized slots
```
