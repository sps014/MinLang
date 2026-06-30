# Memory Model

Dream manages heap memory for you using **Automatic Reference Counting (ARC)** backed by a fast **freelist allocator** running inside WebAssembly.

You don't call `free` and you don't have a garbage collector. Memory is reclaimed the moment the last reference to an object drops.

## What lives on the heap

- Strings
- Arrays (`T[]`)
- Class instances
- Standard library collections (`List`, `Map`)

Primitive values (`int`, `float`, `double`, `bool`) are stored directly on the WASM stack or in locals — no allocation needed.

## Reference counting

Every heap-allocated object has a reference count in its header. The compiler inserts `retain` and `release` calls automatically, distinguishing two kinds of value:

- An **owned** value is freshly produced and already carries exactly one reference: a constructor call (`Point(1, 2)`), a class or array literal, a string concatenation, or the result of a function/method call (a callee hands its result back with `+1`). When you bind, store, or return an owned value, it is *moved* into its new home — no extra retain.
- A **borrowed** value names something another owner already holds: reading a variable, a field, or an array element. Binding or storing a borrowed value retains it (increments the count), since there is now an additional owner.

In both cases:

- When a variable **goes out of scope**, the compiler releases it. If the count reaches zero, the object is freed (and its `del` runs first).
- When an owned **temporary** is used only as a call argument, it is released after the call returns, so it is reclaimed once the callee is done borrowing it.
- Reassigning a variable releases the value it previously held.

You don't write any of this yourself. The upshot is that values reach a reference count of zero and are freed deterministically the moment they are no longer reachable — including the results of factory functions and methods, not just locals.

If a class defines a `del()` [destructor](language/classes.md#destructors), it is called automatically at the moment its reference count reaches zero, just before the block is freed. This is where you put cleanup logic that must run when an instance is destroyed.

```dream
fun make_list(): int[] {
    let arr = [1, 2, 3];   // allocated, ref_count = 1
    return arr;            // retained before locals released, count stays 1
}                          // local `arr` released — count back to 1 (caller holds it)

fun main(): void {
    let result = make_list();   // ref_count = 1
    println(result[0]);
}                               // result goes out of scope -> ref_count 0 -> freed
```

## Heap layout

Each allocation is a contiguous block:

```
[size: 4 bytes][tag: 4 bytes][ref_count: 4 bytes][data ...]
```

The pointer you get back points at `data`. The `tag` identifies the type at runtime (used by `is` and `print`).

## Freelist allocator

When an object is freed, its block is inserted into a singly-linked freelist. The next allocation first scans the freelist for a large enough block. If none fits, a bump pointer advances into fresh memory.

The allocator lives entirely in WASM linear memory — no host calls needed for allocation or deallocation.

## Arrays and dynamic collections

Array backing buffers may contain zeroed slots (for example, capacity beyond the `List` `count`). The release loop for reference-typed arrays skips null slots, so partially-filled buffers are always safe to free.

## Cycles

ARC cannot collect reference cycles. If class `A` holds a reference to `B` and `B` holds a reference to `A`, neither will ever reach a count of zero.

The fix is to break the cycle with a nullable field that you set to `null` before the objects go out of use, or to use a parent-owns-children ownership pattern where children hold no back-reference to the parent.
