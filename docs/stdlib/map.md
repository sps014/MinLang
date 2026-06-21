# Map\<K, V\>

`Map<K, V>` is part of the standard library and is available in every program — no import needed. It is a hash map with average O(1) `put`, `get`, `contains`, and `remove`, and grows automatically as it fills.

## Creating a map

```kotlin
let scores = Map<string, int>();
let cache  = Map<int, string>();
```

## Methods

### put

Inserts or updates the value for `key`.

```kotlin
scores.put("alice", 95);
scores.put("bob", 80);
scores.put("alice", 100);   // overwrites 95
```

### get

Returns the value for `key`. If the key is absent, returns the zero value of `V` (`0` for `int`, `null` for references).

```kotlin
println(scores.get("alice"));   // 100
println(scores.get("nobody"));  // 0
```

### get_or

Returns the value for `key`, or `fallback` if the key is absent.

```kotlin
let v = scores.get_or("nobody", -1);   // -1
```

### contains

Returns `true` if the key is present.

```kotlin
println(scores.contains("bob"));     // true
println(scores.contains("nobody"));  // false
```

### remove

Removes the key and returns `true` if it existed, `false` otherwise.

```kotlin
let removed = scores.remove("bob");   // true
```

### size

Number of key-value pairs currently in the map.

```kotlin
println(scores.size());   // 1
```

## Example

```kotlin
fun main() {
    let freq = Map<string, int>();
    let words = ["the", "cat", "sat", "on", "the", "mat"];
    let i = 0;
    while (i < words.len()) {
        let w = words[i];
        freq.put(w, freq.get(w) + 1);
        i = i + 1;
    }
    println(freq.get("the"));   // 2
    println(freq.get("cat"));   // 1
}
```

## Key types

Any type can be a key as long as `hash_code` and `==` work correctly for it:

- **Primitives** (`int`, `float`, `bool`, `char`) — value equality, deterministic hash.
- **`string`** — content equality (not pointer equality), stable hash.
- **Structs** — identity equality unless `hash_code` and `to_string` are overridden; see [The object type](../language/objects.md).
