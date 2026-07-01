# `Map<K, V>`

`Map<K, V>` is part of the standard library and is available in every program — no import needed. It is an open-addressing hash map with average O(1) `put`, `get`, `contains`, and `remove`, and grows automatically as it fills.

## Creating a map

```dream
let scores = Map<string, int>();
let cache  = Map<int, string>();
```

## Methods

### put

Inserts or updates the value for `key`.

```dream
scores.put("alice", 95);
scores.put("bob", 80);
scores.put("alice", 100);   // overwrites 95
```

### set

An alias for `put` that also powers index-assignment (`map[key] = value`).

```dream
scores.set("carol", 70);
scores["dave"] = 60;         // -> scores.set("dave", 60)
```

### get

Returns the value for `key` as an `Option<V>`: `Some(value)` when present, or `None` when the key is absent.

```dream
println(scores.get("alice").unwrap_or(0));   // 100
println(scores.get("nobody").unwrap_or(0));  // 0 (absent)
```

### get_or

Returns the value for `key`, or `fallback` if the key is absent. A convenience for the common `get(key).unwrap_or(fallback)` pattern.

```dream
let v = scores.get_or("nobody", -1);   // -1
```

### contains

Returns `true` if the key is present.

```dream
println(scores.contains("bob"));     // true
println(scores.contains("nobody"));  // false
```

### remove

Removes the key and returns `true` if it existed, `false` otherwise.

```dream
let removed = scores.remove("bob");   // true
```

### size

Number of key-value pairs currently in the map.

```dream
println(scores.size());   // 1
```

### is_empty

Returns `true` when the map holds no key-value pairs.

```dream
println(scores.is_empty());   // false
```

### clear

Removes every entry and resets the map to its initial empty capacity.

```dream
scores.clear();
println(scores.size());   // 0
```

### keys

Returns a freshly allocated array of every stored key, in unspecified order.

```dream
let ks = scores.keys();   // string[]
```

### values

Returns a freshly allocated array of every stored value, in unspecified order (key-aligned with `keys()` when the map is not mutated between calls).

```dream
let vs = scores.values();   // int[]
```

### iterator

Returns an enumerator so a map can be used directly in a `for..in` loop. Each iteration yields a
`KeyValuePair<K, V>` (with public `key` and `value` fields), in unspecified order. You rarely call
this method by hand — `for (let pair in map)` calls it for you.

```dream
for (let pair in scores) {
    print(pair.key);
    print(" = ");
    println(pair.value);
}
```

## Indexing and iteration

`Map` supports the class [indexer and enumerator conventions](../language/classes.md#indexers-and-enumerators):

```dream
let m = Map<string, int>();
m["a"] = 1;                        // -> m.set("a", 1)
let hit = m["a"];                  // -> m.get("a")  => Option<int>
for (let pair in m) { /* KeyValuePair<string, int> */ }
```

## Example

```dream
fun main() {
    let freq = Map<string, int>();
    let words = ["the", "cat", "sat", "on", "the", "mat"];
    let i = 0;
    while (i < words.len()) {
        let w = words[i];
        freq.put(w, freq.get_or(w, 0) + 1);
        i = i + 1;
    }
    println(freq.get_or("the", 0));   // 2
    println(freq.get_or("cat", 0));   // 1
    println(freq.size());             // 5
}
```

## Key types

Any type can be a key as long as `hash_code` and `==` work correctly for it:

- **Primitives** (`int`, `float`, `bool`, `char`) — value equality, deterministic hash.
- **`string`** — content equality (not pointer equality), stable hash.
- **Classes** — identity equality and default hash unless `hash_code` and `==` are customized; see [The object type](../language/objects.md).
