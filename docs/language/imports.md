# Imports

A MinLang program can be split across multiple `.ml` files. Use `import` at the top of a file to pull in the declarations (functions, structs, enums) from another file.

## Importing a file

```kotlin
import "math_lib.ml"
```

- The path is relative to the file that contains the `import`.
- The `.ml` extension is optional: `import "math_lib"` and `import "math_lib.ml"` are equivalent.
- Imported declarations become directly usable — no namespace prefix.

```kotlin
// math_lib.ml
export fun add_numbers(a: int, b: int): int {
    return a + b;
}
```

```kotlin
// main.ml
import "math_lib.ml"

fun main() {
    println(add_numbers(10, 20));   // 30
}
```

Imports are resolved recursively (an imported file may import others), and each file is processed only once even if imported from several places.

## Export visibility

Mark a declaration `export` to make it part of a file's public surface and to expose it to the host environment. An exported function cannot expose a struct that is not itself exported.

```kotlin
export struct Point {
    x: int;
    y: int;
}

export fun origin(): Point {
    return Point { x: 0, y: 0 };
}
```

## Importing from JavaScript

Pulling in functions from the JavaScript host (rather than another `.ml` file) uses `extern fun` and is covered in [JS Interop](interop.md).
