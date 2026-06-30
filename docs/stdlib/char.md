# char

`char` is a single character (one byte / one code point stored as an `i32`). Write char literals in single quotes: `'A'`, `'\n'`. These methods are available on any `char` value. All are auto-imported — no import needed.

## is_digit

Returns `true` if this character is an ASCII decimal digit (`'0'`–`'9'`).

```dream
println('5'.is_digit());   // true
println('a'.is_digit());   // false
```

## is_alpha

Returns `true` if this character is an ASCII letter (`'a'`–`'z'` or `'A'`–`'Z'`).

```dream
println('A'.is_alpha());   // true
println('3'.is_alpha());   // false
```

## is_whitespace

Returns `true` if this character is ASCII whitespace (space, tab `'\t'`, newline `'\n'`, or carriage return `'\r'`).

```dream
println(' '.is_whitespace());    // true
println('a'.is_whitespace());    // false
```

## to_lower

Returns the lowercase form of an ASCII uppercase letter. Other characters are returned unchanged.

```dream
println('A'.to_lower());   // 'a'
println('5'.to_lower());   // '5'
```

## to_upper

Returns the uppercase form of an ASCII lowercase letter. Other characters are returned unchanged.

```dream
println('a'.to_upper());   // 'A'
```

## to_int

Returns the numeric code point of this character.

```dream
let n = 'A'.to_int();   // 65
```

## as_string

Returns a new single-character string containing this character.

```dream
let s = 'H'.as_string();   // "H"
```
