# Operators

## Arithmetic

| Operator | Meaning         | Types              |
|----------|-----------------|--------------------|
| `+`      | Addition / string concat | `int`, `float`, `double`, `string` |
| `-`      | Subtraction     | `int`, `float`, `double` |
| `*`      | Multiplication  | `int`, `float`, `double` |
| `/`      | Division        | `int`, `float`, `double` |
| `%`      | Remainder       | `int`, `float`     |

Both operands must be the same type. Use an explicit cast if they differ:

```minlang
let x = 7 / (float)2;   // 3.5
```

String concatenation uses `+`:

```minlang
let msg = "Hello, " + name + "!";
```

## Comparison

All comparison operators return `bool`.

| Operator | Meaning                   |
|----------|---------------------------|
| `==`     | Equal                     |
| `!=`     | Not equal                 |
| `<`      | Less than                 |
| `<=`     | Less than or equal        |
| `>`      | Greater than              |
| `>=`     | Greater than or equal     |

String `==` and `!=` compare the **contents** of the strings, not their addresses.

## Logical

| Operator | Meaning     |
|----------|-------------|
| `&&`     | Logical AND |
| `\|\|`     | Logical OR  |
| `!`      | Logical NOT |

Both sides of `&&` and `||` must be `bool`.

## Assignment

`=` assigns a new value to a variable, array element, or struct field:

```minlang
x = 10;
arr[0] = 99;
point.x = 3;
```

## Negation

Prefix `-` negates a numeric value:

```minlang
let neg = -x;
```

## Operator precedence

Higher rows bind tighter:

| Precedence | Operators         |
|------------|-------------------|
| 6          | unary `-`, `!`    |
| 5          | `*`, `/`, `%`     |
| 4          | `+`, `-`          |
| 3          | `<`, `<=`, `>`, `>=` |
| 2          | `==`, `!=`        |
| 1          | `&&`, `\|\|`        |

Use parentheses to make order explicit.
