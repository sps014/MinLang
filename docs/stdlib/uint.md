# uint

`uint` is a 32-bit unsigned integer. Write a `uint` literal with the `u` suffix (`42u`). All operations use unsigned semantics. These methods are auto-imported.

## min / max

Returns the smaller / larger of this value and `other`.

```dream
println((5u).min(3u));    // 3
println((5u).max(3u));    // 5
```

## clamp

Returns this value constrained to the inclusive range `[lo, hi]`.

```dream
println((15u).clamp(0u, 10u));   // 10
```

## uint.parse (static)

Parses an unsigned decimal integer, returning `Result<uint, string>`.

```dream
let n = uint.parse("4000000000").unwrap_or(0u);   // 4000000000
println(uint.parse("abc").is_err());               // true
```
