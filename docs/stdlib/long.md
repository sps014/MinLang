# long

`long` is a 64-bit signed integer. Write a `long` literal with the `L` suffix (`42L`). These methods are available on any `long` value and are auto-imported.

## abs

Returns the absolute value.

```dream
println((0L - 7L).abs());   // 7
```

## min / max

Returns the smaller / larger of this value and `other`.

```dream
println((5L).min(3L));    // 3
println((5L).max(3L));    // 5
```

## clamp

Returns this value constrained to the inclusive range `[lo, hi]`.

```dream
println((15L).clamp(0L, 10L));   // 10
```

## signum

Returns the sign: `-1` for negative, `0` for zero, `1` for positive.

```dream
println((-42L).signum());   // -1
```

## long.parse (static)

Parses a signed decimal integer, returning `Result<long, string>`.

```dream
let n = long.parse("9000000000").unwrap_or(0L);   // 9000000000
println(long.parse("abc").is_err());               // true
```
