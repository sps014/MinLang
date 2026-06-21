# Built-in Functions

These are available in every MinLang program without any import.

## print

Prints a value to stdout without a trailing newline. Works on all types.

```kotlin
print(42);         // prints "42"
print(3.14f);      // prints "3.14"
print("hello");    // prints "hello"
print(true);       // prints "true"
print('A');        // prints "A"
```

For structs that override `to_string`, `print` calls the override automatically.

## println

Like `print`, but appends a newline (`\n`) after the value.

```kotlin
println(42);       // prints "42\n"
println("hello");  // prints "hello\n"
```

## to_string

Converts any value to its string representation:

```kotlin
let s = to_string(42);      // "42"
let b = to_string(true);    // "true"
let f = to_string(3.14f);   // "3.14"
```

For structs with a `@override pub fun to_string()` method, that method is called.

## hash_code

Returns a stable `int` hash for any value:

```kotlin
let h = hash_code("hello");
let h2 = hash_code(42);
```

Used internally by `Map<K, V>` to find buckets.

## Math

Math functions live in the `Math` namespace and are called with `Math.<name>(x)`.

| Function     | Signature          | Description     |
|--------------|--------------------|-----------------|
| `Math.sin`   | `(float) -> float` | Sine (radians)  |
| `Math.cos`   | `(float) -> float` | Cosine (radians)|
| `Math.sqrt`  | `(float) -> float` | Square root     |
| `Math.abs`   | `(float) -> float` | Absolute value  |

```kotlin
let hyp = Math.sqrt(3.0f * 3.0f + 4.0f * 4.0f);   // 5.0
```

## len

`len` is a method on arrays and strings:

```kotlin
let arr = [10, 20, 30];
println(arr.len());     // 3

let name = "hello";
println(name.len());    // 5
```

## array_new

Allocates a zeroed array of a given size. Mainly used by the standard library internals. You can use it in your own code when you need an array whose size isn't known at compile time:

```kotlin
let buf = array_new<int>(100);   // int[] with 100 zero-initialized slots
```
