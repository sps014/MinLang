# Imports

A Dream program can be split across multiple `.dream` files. Use `import` at the top of a file to pull in the declarations (functions, classes, enums) from another file.

## Importing a file

```dream
import math_lib;
```

- The path is a dotted module path (identifiers separated by `.`) ending with a semicolon.
- Each `.` maps to a directory separator, and the `.dream` extension is added automatically: `import utils.math_lib;` resolves to `utils/math_lib.dream`.
- The path is relative to the file that contains the `import`.
- Imported declarations become directly usable — no namespace prefix.

```dream
// math_lib.dream
public fun add_numbers(a: int, b: int): int {
    return a + b;
}
```

```dream
// main.dream
import math_lib;

fun main() {
    println(add_numbers(10, 20));   // 30
}
```

Imports are resolved recursively (an imported file may import others), and each file is processed only once even if imported from several places.

## Visibility

Declarations are **private by default**: visible throughout their own module but not exposed to the host. Mark a declaration `public` to make it part of a file's public surface and to expose it to the host environment. A `public` function cannot expose a class that is not itself `public`.

```dream
public class Point {
    public x: int;
    public y: int;
}

public fun origin(): Point {
    return Point(0, 0);
}
```

## Importing from JavaScript

Pulling in functions from the JavaScript host (rather than another `.dream` file) uses `extern fun` and is covered in [JS Interop](interop.md).
