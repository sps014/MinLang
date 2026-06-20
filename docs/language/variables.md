# Variables

## Declaring a variable

Use `let` to declare a local variable. The type is inferred from the right-hand side:

```minlang
let x = 42;          // int
let name = "Alice";  // string
let ratio = 3.14;    // float
let done = false;    // bool
```

You can also write the type explicitly. This is required when the initializer alone is ambiguous:

```minlang
let score: double = 99.5d;
let items: int[] = [1, 2, 3];
```

## Assignment

Variables are mutable by default. Assign a new value with `=`:

```minlang
let count = 0;
count = count + 1;
```

## Scope

Variables live until the end of the block they are declared in. When a reference-typed variable (string, array, struct) goes out of scope, its reference count is decremented automatically.

```minlang
fun main(): void {
    let a = 10;
    {
        let b = 20;        // b is only alive here
        print(a + b);
        print("\n");
    }
    // b is gone here; a is still fine
}
```

## Type inference rules

The compiler infers the type from the initializer expression. A few things to watch out for:

- Number literals without a suffix are `int`.
- Literals ending in `f`/`F` are `float` (`3.14f`).
- Literals ending in `d`/`D` are `double` (`3.14d`).
- Literals with a `.` but no suffix are also `float`.
- String literals are `string`.

If inference gives you the wrong type, add an explicit annotation or a suffix:

```minlang
let pi: double = 3.14159;   // explicit annotation
let pi2 = 3.14159d;         // suffix
```
