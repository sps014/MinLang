# string

`string` is a built-in reference type (heap-allocated, null-terminated UTF-8). It is available in every program with no import. These methods are available on any string value.

## len

Returns the number of characters. This is the same as `length()`.

```ts
let n = "hello".len();   // 5
```

## length

Alias for `len()`.

```ts
let n = "hello".length();   // 5
```

## is_empty

Returns `true` when the string has no characters.

```ts
println("".is_empty());       // true
println("hi".is_empty());     // false
```

## char_at

Returns the character at `index`. No bounds checking.

```ts
let c = "hello".char_at(1);   // 'e'
```

## substring

Returns a new string containing the characters in the half-open range `[start, end)`. A non-positive length yields the empty string.

```ts
let s = "hello world".substring(6, 11);   // "world"
```

## index_of

Returns the index of the first occurrence of character `target`, or `-1` if absent.

```ts
let i = "hello".index_of('l');   // 2
let j = "hello".index_of('z');   // -1
```

## contains

Returns `true` if `sub` occurs anywhere in the string. The empty string is always contained.

```ts
println("hello world".contains("world"));   // true
println("hello world".contains("xyz"));     // false
```

## starts_with

Returns `true` if the string begins with `prefix`.

```ts
println("hello".starts_with("hel"));   // true
```

## ends_with

Returns `true` if the string ends with `suffix`.

```ts
println("hello".ends_with("llo"));   // true
```

## to_lower

Returns a new string with every ASCII uppercase letter lowercased.

```ts
println("Hello World".to_lower());   // "hello world"
```

## to_upper

Returns a new string with every ASCII lowercase letter uppercased.

```ts
println("Hello World".to_upper());   // "HELLO WORLD"
```

## trim

Returns a new string with leading and trailing ASCII whitespace removed.

```ts
println("  hello  ".trim());   // "hello"
```

## repeat

Returns a new string consisting of the original repeated `times` times. A count of `0` or less yields the empty string.

```ts
println("ab".repeat(3));   // "ababab"
```

## equals

Returns `true` if this string has the same contents as `other`. This is identical to using `==`.

```ts
println("hello".equals("hello"));   // true
```
