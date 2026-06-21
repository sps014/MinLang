# Imports

A Dream program can be split across multiple `.dream` files. Use `import` at the top of a file to pull in the declarations (functions, structs, enums) from another file.

## Importing a file

```kotlin
import "math_lib.dream"
```

- The path is relative to the file that contains the `import`.
- The `.dream` extension is optional: `import "math_lib"` and `import "math_lib.dream"` are equivalent.
- Imported declarations become directly usable — no namespace prefix.

```kotlin
// math_lib.dream
pub fun add_numbers(a: int, b: int): int {
    return a + b;
}
```

```kotlin
// main.dream
import "math_lib.dream"

fun main() {
    println(add_numbers(10, 20));   // 30
}
```

Imports are resolved recursively (an imported file may import others), and each file is processed only once even if imported from several places.

## Export visibility

Mark a declaration `pub` to make it part of a file's public surface and to expose it to the host environment. An exported function cannot expose a struct that is not itself exported.

```kotlin
pub struct Point {
    x: int;
    y: int;
}

pub fun origin(): Point {
    return Point { x: 0, y: 0 };
}
```

## Importing from JavaScript

Pulling in functions from the JavaScript host (rather than another `.dream` file) uses `extern fun` and is covered in [JS Interop](interop.md).
