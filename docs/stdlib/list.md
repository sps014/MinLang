# List<T>

`List<T>` is part of the standard library and is available in every program — no import needed. It is a growable sequence of values of type `T` with O(1) random access and amortized O(1) `push`.

## Creating a list

```ts
let nums = List<int>();
let words = List<string>();
```

## Methods

### push

Appends a value to the end, growing the backing buffer if needed.

```ts
nums.push(10);
nums.push(20);
nums.push(30);
```

### size

Number of elements currently in the list.

```ts
println(nums.size());   // 3
```

### get

Returns the element at `index`. No bounds checking — going out of range is undefined behaviour.

```ts
println(nums.get(0));   // 10
```

### set

Overwrites the element at `index`.

```ts
nums.set(1, 99);
println(nums.get(1));   // 99
```

### pop

Removes and returns the last element. Does not check if the list is empty.

```ts
let last = nums.pop();
```

### contains

Returns `true` if the value is present. Uses value equality (string contents, not pointers).

```ts
println(nums.contains(99));    // true
println(nums.contains(1000));  // false
```

### index_of

Returns the index of the first matching element, or `-1` if not found.

```ts
let i = nums.index_of(99);   // 1 (or -1 if absent)
```

### clear

Resets the element count to zero.

```ts
nums.clear();
println(nums.size());   // 0
```

### remove_at

Removes the element at `index`, shifting everything after it left.

```ts
nums.remove_at(0);   // removes the first element
```

## Example

```ts
fun main() {
    let xs = List<int>();
    let i = 0;
    while (i < 5) {
        xs.push(i * i);
        i = i + 1;
    }
    // [0, 1, 4, 9, 16]
    println(xs.size());          // 5
    println(xs.get(4));          // 16
    println(xs.contains(9));     // true
    xs.remove_at(2);             // [0, 1, 9, 16]
    println(xs.size());          // 4
}
```

A `List<T>` grows automatically from a small initial capacity; you never resize it manually.
