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
