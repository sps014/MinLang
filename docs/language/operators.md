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

```ts
let x = 7 / (float)2;   // 3.5
```

String concatenation uses `+`:

```ts
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

Both sides of `&&` and `||` must be `bool`. `&&` and `||` **short-circuit**: the right operand is only evaluated when it can still affect the result.

## Bitwise

These operate on `int` values:

| Operator | Meaning       |
|----------|---------------|
| `&`      | Bitwise AND   |
| `\|`      | Bitwise OR    |
| `^`      | Bitwise XOR   |
| `<<`     | Shift left    |
| `>>`     | Shift right (arithmetic) |

## Null-coalescing

`a ?? b` evaluates to `a` when it is non-null, otherwise to `b`. The left operand should be a nullable (`T?`) value and the result type is the unwrapped `T`:

```ts
let name: string? = lookup();
let display: string = name ?? "anonymous";
```

## Ternary

`cond ? a : b` evaluates `cond` (a `bool`); the result is `a` when true and `b` when false. Both branches must share a type:

```ts
let label = score >= 60 ? "pass" : "fail";
```

## Assignment

`=` assigns a new value to a variable, array element, or class field:

```ts
x = 10;
arr[0] = 99;
point.x = 3;
```

### Compound assignment and increment

`+=`, `-=`, `*=`, `/=`, and `%=` update a target in place, and `++`/`--` add or subtract one:

```ts
total += 5;     // total = total + 5
count++;        // count = count + 1
i--;
```

## Negation

Prefix `-` negates a numeric value:

```ts
let neg = -x;
```

## Operator precedence

Higher rows bind tighter:

| Precedence | Operators         |
|------------|-------------------|
| unary      | unary `-`, `!`    |
| highest    | `&`               |
|            | `^`               |
|            | `\|`               |
|            | `%`               |
|            | `*`, `/`          |
|            | `+`, `-`          |
|            | `<<`, `>>`        |
|            | `<`, `<=`, `>`, `>=`, `==`, `!=`, `is` |
|            | `&&`              |
|            | `\|\|`             |
| lowest     | `??`, then `? :`  |

Use parentheses to make order explicit.
