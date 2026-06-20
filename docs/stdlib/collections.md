# Collections

`List<T>` and `Map<K, V>` are part of the standard library and are available in every program — no import needed.

Both are generic structs that grow dynamically. The compiler generates a dedicated, allocation-free version for each concrete type combination you use.

---

## List\<T\>

A growable sequence of values of type `T`, backed by a doubling array. Random access in O(1); push amortized O(1).

### Creating a list

```kotlin
let nums = List<int>();
let words = List<string>();
```

### Methods

#### push

Appends a value to the end. Grows the backing buffer if needed.

```kotlin
nums.push(10);
nums.push(20);
nums.push(30);
```

#### size

Number of elements currently in the list.

```kotlin
print(nums.size());   // 3
```

#### get

Returns the element at `index`. No bounds checking — going out of range is undefined behaviour.

```kotlin
print(nums.get(0));   // 10
```

#### set

Overwrites the element at `index`.

```kotlin
nums.set(1, 99);
print(nums.get(1));   // 99
```

#### pop

Removes and returns the last element. Does not check if the list is empty.

```kotlin
let last = nums.pop();
```

#### contains

Returns `true` if the value is present. Uses value equality (string contents, not pointers).

```kotlin
print(nums.contains(99));    // true
print(nums.contains(1000));  // false
```

#### index_of

Returns the index of the first matching element, or `-1` if not found.

```kotlin
let i = nums.index_of(99);   // 1 (or -1 if absent)
```

#### clear

Resets the element count to zero. Does not resize the backing buffer.

```kotlin
nums.clear();
print(nums.size());   // 0
```

#### remove_at

Removes the element at `index` by shifting everything after it left. O(n) in the worst case.

```kotlin
nums.remove_at(0);   // removes the first element
```

### Example

```kotlin
fun main(): void {
    let xs = List<int>();
    let i = 0;
    while (i < 5) {
        xs.push(i * i);
        i = i + 1;
    }
    // [0, 1, 4, 9, 16]
    print(xs.size());          // 5
    print(xs.get(4));          // 16
    print(xs.contains(9));     // true
    print("\n");
    xs.remove_at(2);           // [0, 1, 9, 16]
    print(xs.size());          // 4
}
```

---

## Map\<K, V\>

An open-addressing hash map with linear probing. Average O(1) for put, get, contains, and remove. Automatically rehashes when the load factor exceeds ~75%.

### Creating a map

```kotlin
let scores = Map<string, int>();
let cache  = Map<int, string>();
```

### Methods

#### put

Inserts or updates the value for `key`.

```kotlin
scores.put("alice", 95);
scores.put("bob", 80);
scores.put("alice", 100);   // overwrites 95
```

#### get

Returns the value for `key`. If the key is absent, returns the zero value of `V` (`0` for `int`, `null` for references).

```kotlin
print(scores.get("alice"));   // 100
print(scores.get("nobody"));  // 0
```

#### get_or

Returns the value for `key`, or `fallback` if the key is absent.

```kotlin
let v = scores.get_or("nobody", -1);   // -1
```

#### contains

Returns `true` if the key is present.

```kotlin
print(scores.contains("bob"));     // true
print(scores.contains("nobody"));  // false
```

#### remove

Removes the key and returns `true` if it existed, `false` otherwise.

```kotlin
let removed = scores.remove("bob");   // true
```

#### size

Number of key-value pairs currently in the map.

```kotlin
print(scores.size());   // 1 (only alice after removing bob)
```

### Example

```kotlin
fun main(): void {
    let freq = Map<string, int>();
    let words = ["the", "cat", "sat", "on", "the", "mat"];
    let i = 0;
    while (i < len(words)) {
        let w = words[i];
        freq.put(w, freq.get(w) + 1);
        i = i + 1;
    }
    print(freq.get("the"));   // 2
    print(freq.get("cat"));   // 1
}
```

### Key types

Any type can be a key as long as `hash_code` and `==` work correctly for it:

- **Primitives** (`int`, `float`, `bool`) — value equality, deterministic hash.
- **`string`** — content equality (not pointer equality), stable hash.
- **Structs** — identity equality (pointer comparison) unless `hash_code` and `to_string` are overridden; see [The object type](../language/objects.md).

---

## How growth works

Both collections start with a small initial capacity (8 for `List`, 8 buckets for `Map`) and expand automatically. You never call any resize method manually. Old backing memory is reclaimed by the ARC system as soon as it is replaced.
