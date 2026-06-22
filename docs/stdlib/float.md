# float

`float` is a 32-bit IEEE 754 floating-point number. Write float literals with an `f` suffix (`3.14f`) or with a decimal point and no suffix (`3.14`). These methods are available on any `float` value. All are auto-imported — no import needed.

## abs

Returns the absolute value.

```c
println((-1.5f).abs());   // 1.5
```

## min

Returns the smaller of this value and `other`.

```c
println(2.0f.min(5.0f));   // 2.0
```

## max

Returns the larger of this value and `other`.

```c
println(2.0f.max(5.0f));   // 5.0
```
