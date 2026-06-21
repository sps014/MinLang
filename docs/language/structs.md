# Structs

Structs are user-defined types that group related data together.

## Defining a struct

```kotlin
struct Point {
    x: int;
    y: int;
}
```

Fields are declared as `name: type;` pairs. A struct literal must provide every field; a constructor with a custom `pub init` may leave fields unset, in which case they start at their zero value (see [Constructors](#constructors)).

## Creating an instance

There are two ways to create an instance.

Using a struct literal, naming each field (fields can be provided in any order):

```kotlin
let p = Point { x: 3, y: 4 };
```

Or using a constructor call, passing values positionally:

```kotlin
let p = Point(3, 4);
```

## Constructors

Every struct has an **auto-generated constructor** that accepts its fields in declaration
order. For `Point` above, that is `Point(x, y)`.

To run custom logic when an instance is created, define a constructor with `pub init(...)`.
When an `init` is present, the constructor call matches its parameters instead of the fields,
and any field you do not assign starts at its zero value (`0`, `0.0`, `false`, or `null`):

```kotlin
struct Account {
    owner: string;
    balance: int;

    pub init(owner: string) {
        this.owner = owner;
        this.balance = 100;   // default starting balance
    }
}

fun main(): void {
    let a = Account("Ada");
    println(a.balance);       // 100
}
```

An `init` declares no return type — it always produces an instance of its struct. Inside the
body, `this` refers to the new instance.

## Accessing and mutating fields

Use `.`:

```kotlin
println(p.x);      // 3
p.x = 10;
println(p.x);      // 10
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
    println(c.get());   // 2
}
```

Methods are called with `instance.method(args)`. The `this` parameter is implicit — you do not pass it yourself.

## Destructors

Define `pub drop()` to run cleanup logic when an instance is destroyed. A struct is destroyed
when its last reference goes out of scope; `drop` runs automatically just before the memory is
released, while the fields are still valid. A destructor takes no parameters and has no return
type:

```kotlin
struct File {
    name: string;

    pub init(name: string) {
        this.name = name;
    }

    pub drop() {
        print("closing ");
        println(this.name);
    }
}

fun use_file() {
    let f = File("data.txt");
    // ... work with f ...
}                                  // f goes out of scope here -> "closing data.txt"
```

You never call `drop` yourself; the runtime invokes it as part of automatic memory management.

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

Mark a struct `pub` to make it visible to the WebAssembly host:

```kotlin
pub struct Vec2 {
    x: float;
    y: float;
}
```

## Object protocol overrides

Structs can customize how they are printed and hashed by overriding `to_string` and `hash_code`. See [The object type](objects.md) for details.

## Memory

Each struct instance is a heap allocation. The memory is freed automatically when the last reference to it drops — no manual `free` needed. If the struct defines a `pub drop()` destructor, it runs just before the memory is released. See [Memory Model](../memory.md) for a full explanation.
