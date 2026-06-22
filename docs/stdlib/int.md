# int

`int` is a 32-bit signed integer. These methods are available on any `int` value. All are auto-imported — no import needed.

## abs

Returns the absolute value.

```c
println((-7).abs());   // 7
println(3.abs());      // 3
```

## min

Returns the smaller of this value and `other`.

```c
println(5.min(3));    // 3
println(2.min(10));   // 2
```

## max

Returns the larger of this value and `other`.

```c
println(5.max(3));    // 5
println(2.max(10));   // 10
```

## clamp

Returns this value constrained to the inclusive range `[lo, hi]`.

```c
println(15.clamp(0, 10));    // 10
println((-5).clamp(0, 10));  // 0
println(7.clamp(0, 10));     // 7
```

## pow

Returns this value raised to a non-negative integer power. Exponents of `0` or less yield `1`.

```c
println(2.pow(10));   // 1024
println(3.pow(3));    // 27
```

## signum

Returns the sign: `-1` for negative, `0` for zero, `1` for positive.

```c
println((-42).signum());   // -1
println(0.signum());       // 0
println(7.signum());       // 1
```

## int.parse (static)

Parses a signed decimal integer from a string. Non-digit characters are ignored; an empty or all-non-digit string yields `0`.

```c
let n = int.parse("42");     // 42
let m = int.parse("-7");     // -7
let k = int.parse("abc");    // 0
```
