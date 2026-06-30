# Built-in Functions

These are available in every Dream program without any import.

## System.print

Prints a value to stdout without a trailing newline. Works on all types.

```dream
System.print(42);         // prints "42"
System.print(3.14f);      // prints "3.14"
System.print("hello");    // prints "hello"
System.print(true);       // prints "true"
System.print('A');        // prints "A"
```

For classes that override `to_string`, `System.print` calls the override automatically. Because
`print`/`println` are generic over the value type, you never need to convert first — pass the value
directly.

## System.println

Like `System.print`, but appends a newline (`\n`) after the value.

```dream
System.println(42);       // prints "42\n"
System.println("hello");  // prints "hello\n"
```

## to_string

`to_string()` is a universal instance method available on every value, returning its string
representation:

```dream
let s = (42).to_string();      // "42"
let b = (true).to_string();    // "true"
let f = (3.14f).to_string();   // "3.14"
```

For classes with a `@override public fun to_string()` method, that method is called.

You rarely need to call it explicitly: `System.print`/`println` already convert any value, and
string concatenation auto-converts the non-string operand, so `"x = " + 42` yields `"x = 42"`.

## hash_code

`hash_code()` is a universal instance method returning a stable `int` hash for any value:

```dream
let h = "hello".hash_code();
let h2 = (42).hash_code();
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

```dream
let hyp = Math.sqrt(3.0 * 3.0 + 4.0 * 4.0);         // 5.0
let s = Math.sin(0);                                // 0.0 (int argument coerced to double)
let p = Math.pow(2.0, 3.0);                         // 8.0
```

## len

`len` is a method on arrays and strings:

```dream
let arr = [10, 20, 30];
System.println(arr.len());     // 3

let name = "hello";
System.println(name.len());    // 5
```

## Array.new

Allocates a zeroed array of a given size. Mainly used by the standard library internals. You can use it in your own code when you need an array whose size isn't known at compile time:

```dream
let buf = Array.new<int>(100);   // int[] with 100 zero-initialized slots
```
