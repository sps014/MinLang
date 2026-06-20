# Arrays

## Creating an array

Write a comma-separated list of values inside `[...]`. All elements must be the same type:

```kotlin
let nums = [1, 2, 3, 4, 5];       // int[]
let words = ["red", "green", "blue"]; // string[]
```

## Reading and writing elements

Zero-indexed bracket access:

```kotlin
let first = nums[0];   // 1
nums[2] = 99;          // [1, 2, 99, 4, 5]
```

Going out of bounds is undefined behaviour — there is no automatic bounds check at runtime.

## Array length

Use `len` to get the number of elements:

```kotlin
let count = len(nums);   // 5
```

## Passing arrays to functions

Arrays are reference types. Passing an array to a function does not copy it; both the caller and the callee see the same backing buffer:

```kotlin
fun fill_zeros(arr: int[]): void {
    let i = 0;
    while (i < len(arr)) {
        arr[i] = 0;
        i = i + 1;
    }
}
```

## Fixed size

Arrays created from literals are fixed-size. You cannot push or pop from them.

If you need a growable array, use [`List<T>`](../stdlib/collections.md):

```kotlin
let xs = List<int>();
xs.push(10);
xs.push(20);
print(xs.size());   // 2
```

## Array of structs

```kotlin
struct Point { x: int; y: int; }

let pts: Point[] = [
    Point { x: 0, y: 0 },
    Point { x: 1, y: 2 },
];
print(pts[1].x);   // 1
```
