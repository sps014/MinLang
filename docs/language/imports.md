# Imports

A Dream program can be split across multiple `.dream` files. Use `import` at the top of a file to pull in the declarations (functions, classes, enums) from another file.

## Importing a file

```ts
import "math_lib.dream"
```

- The path is relative to the file that contains the `import`.
- The `.dream` extension is optional: `import "math_lib"` and `import "math_lib.dream"` are equivalent.
- Imported declarations become directly usable — no namespace prefix.

```ts
// math_lib.dream
export fun add_numbers(a: int, b: int): int {
    return a + b;
}
```

```ts
// main.dream
import "math_lib.dream"

fun main() {
    println(add_numbers(10, 20));   // 30
}
```

Imports are resolved recursively (an imported file may import others), and each file is processed only once even if imported from several places.

## Export visibility

Mark a declaration `export` to make it part of a file's public surface and to expose it to the host environment. An exported function cannot expose a class that is not itself exported.

```ts
export class Point {
    x: int;
    y: int;
}

export fun origin(): Point {
    return Point { x: 0, y: 0 };
}
```

## Importing from JavaScript

Pulling in functions from the JavaScript host (rather than another `.dream` file) uses `extern fun` and is covered in [JS Interop](interop.md).
