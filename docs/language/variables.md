# Variables

## Declaring a variable

Use `let` to declare a local variable. The type is inferred from the right-hand side:

```ts
let x = 42;          // int
let name = "Alice";  // string
let ratio = 3.14;    // float
let done = false;    // bool
```

You can also write the type explicitly. This is required when the initializer alone is ambiguous:

```ts
let score: double = 99.5d;
let items: int[] = [1, 2, 3];
```

## Constants

Use `const` instead of `let` to declare an immutable binding. Reassigning a `const` is a compile error:

```ts
const pi: int = 3;
// pi = 4;   // error: cannot assign to 'pi' because it is a const binding
```

## Assignment

Variables declared with `let` are mutable. Assign a new value with `=`:

```ts
let count = 0;
count = count + 1;
```

Compound assignment (`+=`, `-=`, `*=`, `/=`, `%=`) and increment/decrement (`++`, `--`) are also supported (see [operators](operators.md)).

## Scope

Variables live until the end of the block they are declared in. When a reference-typed variable (string, array, class) goes out of scope, its reference count is decremented automatically.

```ts
fun main(): void {
    let a = 10;
    {
        let b = 20;        // b is only alive here
        println(a + b);
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

```ts
let pi: double = 3.14159;   // explicit annotation
let pi2 = 3.14159d;         // suffix
```
