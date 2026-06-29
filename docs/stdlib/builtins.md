# Built-in Functions

These are available in every Dream program without any import.

## System.print

Prints a value to stdout without a trailing newline. Works on all types.

```ts
System.print(42);         // prints "42"
System.print(3.14f);      // prints "3.14"
System.print("hello");    // prints "hello"
System.print(true);       // prints "true"
System.print('A');        // prints "A"
```

For classes that override `to_string`, `System.print` calls the override automatically.

## System.println

Like `System.print`, but appends a newline (`\n`) after the value.

```ts
System.println(42);       // prints "42\n"
System.println("hello");  // prints "hello\n"
```

## to_string

Converts any value to its string representation:

```ts
let s = to_string(42);      // "42"
let b = to_string(true);    // "true"
let f = to_string(3.14f);   // "3.14"
```

For classes with a `@override export fun to_string()` method, that method is called.

## hash_code

Returns a stable `int` hash for any value:

```ts
let h = hash_code("hello");
let h2 = hash_code(42);
```

Used internally by `Map<K, V>` to find buckets.

## Math

Math functions are static methods on the `Math` class. Each function accepts numeric arguments (coerced to `double`) and always returns `double`.

| Function     | Description      |
|--------------|------------------|
| `Math.sin`   | Sine (radians)   |
| `Math.cos`   | Cosine (radians) |
| `Math.tan`   | Tangent (radians)|
| `Math.sqrt`  | Square root      |
| `Math.abs`   | Absolute value   |
| `Math.pow`   | Power (x^y)      |
| `Math.floor` | Floor            |
| `Math.ceil`  | Ceiling          |
| `Math.round` | Round to nearest |

```ts
let hyp = Math.sqrt(3.0 * 3.0 + 4.0 * 4.0);         // 5.0
let s = Math.sin(0);                                // 0.0 (int argument coerced to double)
let p = Math.pow(2.0, 3.0);                         // 8.0
```

## len

`len` is a method on arrays and strings:

```ts
let arr = [10, 20, 30];
System.println(arr.len());     // 3

let name = "hello";
System.println(name.len());    // 5
```

## array_new

Allocates a zeroed array of a given size. Mainly used by the standard library internals. You can use it in your own code when you need an array whose size isn't known at compile time:

```ts
let buf = array_new<int>(100);   // int[] with 100 zero-initialized slots
```
