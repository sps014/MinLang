# double

`double` is a 64-bit IEEE 754 floating-point number. Write double literals with a `d` suffix (`3.14d`). These methods are available on any `double` value. All are auto-imported — no import needed.

## abs

Returns the absolute value.

```dream
println((-1.5d).abs());   // 1.5
```

## min

Returns the smaller of this value and `other`.

```dream
println(2.0d.min(5.0d));   // 2.0
```

## max

Returns the larger of this value and `other`.

```dream
println(2.0d.max(5.0d));   // 5.0
```

## double.parse (static)

Parses a decimal `double` from a string, supporting an optional sign, a fractional part, and an `e`/`E` exponent. Returns a `Result<double, string>`: `Ok(value)` on success, or `Err(message)` for an empty string or one containing no digits. Use `unwrap_or` (or `switch`) to read the value.

```dream
let x = double.parse("3.14").unwrap_or(0.0d);    // 3.14
let y = double.parse("-1.5e2").unwrap_or(0.0d);  // -150.0
let z = double.parse("nope").unwrap_or(0.0d);    // 0.0 (Err -> fallback)
```
