# ulong

`ulong` is a 64-bit unsigned integer. Write a `ulong` literal with the `uL` suffix (`42uL`). All operations use unsigned semantics. These methods are auto-imported.

## min / max

Returns the smaller / larger of this value and `other`.

```dream
println((5uL).min(3uL));    // 3
println((5uL).max(3uL));    // 5
```

## clamp

Returns this value constrained to the inclusive range `[lo, hi]`.

```dream
println((200uL).clamp(0uL, 100uL));   // 100
```

## ulong.parse (static)

Parses an unsigned decimal integer, returning `Result<ulong, string>`.

```dream
let n = ulong.parse("18000000000").unwrap_or(0uL);   // 18000000000
println(ulong.parse("abc").is_err());                 // true
```
