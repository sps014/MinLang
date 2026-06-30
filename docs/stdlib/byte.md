# byte

`byte` is an 8-bit unsigned integer in the range `0–255`, the element type for raw binary data (`byte[]`). Write a `byte` literal with the `b` suffix (`255b`). These methods are auto-imported.

## min / max

Returns the smaller / larger of this value and `other`.

```dream
println((5b).min(3b));    // 3
println((5b).max(3b));    // 5
```

## clamp

Returns this value constrained to the inclusive range `[lo, hi]`.

```dream
println((200b).clamp(0b, 100b));   // 100
```

## byte.parse (static)

Parses an unsigned decimal byte (`0–255`), returning `Result<byte, string>`. Values outside the range produce `Err`.

```dream
let n = byte.parse("200").unwrap_or(0b);   // 200
println(byte.parse("300").is_err());        // true (out of range)
```
