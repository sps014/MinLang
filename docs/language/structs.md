# Structs

Structs are user-defined types that group related data together.

## Defining a struct

```kotlin
struct Point {
    x: int;
    y: int;
}
```

Fields are declared as `name: type;` pairs. There are no default values; every field must be provided when creating an instance.

## Creating an instance

```kotlin
let p = Point { x: 3, y: 4 };
```

Fields can be provided in any order.

## Accessing and mutating fields

Use `.`:

```kotlin
print(p.x);      // 3
p.x = 10;
print(p.x);      // 10
```

## Methods

Define methods inside the struct body using `fun`. Methods automatically receive a `this` parameter that refers to the current instance:

```kotlin
struct Counter {
    count: int;

    fun increment(): void {
        this.count = this.count + 1;
    }

    fun get(): int {
        return this.count;
    }
}

fun main(): void {
    let c = Counter { count: 0 };
    c.increment();
    c.increment();
    print(c.get());   // 2
}
```

Methods are called with `instance.method(args)`. The `this` parameter is implicit — you do not pass it yourself.

## Nullable structs

Append `?` to a struct type to allow `null`:

```kotlin
struct Node {
    value: int;
    next: Node?;
}

let head: Node? = null;
head = Node { value: 1, next: null };
```

## Recursive structs

A struct can hold a nullable reference to itself (non-nullable self-references would have infinite size):

```kotlin
struct Node {
    value: int;
    next: Node?;

    fun has_next(): bool {
        return this.next != null;
    }
}
```

## Exporting structs

Mark a struct `export` to make it visible to the WebAssembly host:

```kotlin
export struct Vec2 {
    x: float;
    y: float;
}
```

## Object protocol overrides

Structs can customize how they are printed and hashed by overriding `to_string` and `hash_code`. See [The object type](objects.md) for details.

## Memory

Each struct instance is a heap allocation. The memory is freed automatically when the last reference to it drops — no manual `free` needed. See [Memory Model](../memory.md) for a full explanation.
