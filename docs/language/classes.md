# Classes

Classes are user-defined types that group related data together.

## Defining a class

```ts
class Point {
    x: int;
    y: int;
}
```

Fields are declared as `name: type;` pairs. A custom `constructor` may leave fields unset, in which case they start at their zero value (see [Constructors](#constructors)).

## Creating an instance

Create an instance with a constructor call, passing values positionally in field declaration order:

```ts
let p = Point(3, 4);
```

Every class is constructed this way; there is no separate brace-literal (`Point { x: 3, y: 4 }`) syntax.

## Constructors

Every class has an **auto-generated constructor** that accepts its fields in declaration
order. For `Point` above, that is `Point(x, y)`.

To run custom logic when an instance is created, define a `constructor(...)`.
When a `constructor` is present, the constructor call matches its parameters instead of the fields,
and any field you do not assign starts at its zero value (`0`, `0.0`, `false`, or `null`):

```ts
class Account {
    owner: string;
    balance: int;

    constructor(owner: string) {
        this.owner = owner;
        this.balance = 100;   // default starting balance
    }
}

fun main(): void {
    let a = Account("Ada");
    println(a.balance);       // 100
}
```

A `constructor` declares no return type — it always produces an instance of its class. Inside the
body, `this` refers to the new instance. A `constructor` cannot be marked `export`.

## Accessing and mutating fields

Use `.`:

```ts
println(p.x);      // 3
p.x = 10;
println(p.x);      // 10
```

## Methods

Define methods inside the class body using `fun`. Methods automatically receive a `this` parameter that refers to the current instance:

```ts
class Counter {
    count: int;

    fun increment(): void {
        this.count = this.count + 1;
    }

    fun get(): int {
        return this.count;
    }
}

fun main(): void {
    let c = Counter(0);
    c.increment();
    c.increment();
    println(c.get());   // 2
}
```

Methods are called with `instance.method(args)`. The `this` parameter is implicit — you do not pass it yourself.

## Destructors

Define `del()` to run cleanup logic when an instance is destroyed. A class is destroyed
when its last reference goes out of scope; `del` runs automatically just before the memory is
released, while the fields are still valid. A destructor takes no parameters and has no return
type, and cannot be marked `export`:

```ts
class File {
    name: string;

    constructor(name: string) {
        this.name = name;
    }

    del() {
        print("closing ");
        println(this.name);
    }
}

fun use_file() {
    let f = File("data.txt");
    // ... work with f ...
}                                  // f goes out of scope here -> "closing data.txt"
```

You never call `del` yourself; the runtime invokes it as part of automatic memory management.

## Nullable classes

Append `?` to a class type to allow `null`:

```ts
class Node {
    value: int;
    next: Node?;
}

let head: Node? = null;
head = Node(1, null);
```

## Recursive classes

A class can hold a nullable reference to itself (non-nullable self-references would have infinite size):

```ts
class Node {
    value: int;
    next: Node?;

    fun has_next(): bool {
        return this.next != null;
    }
}
```

## Exporting classes

Mark a class `export` to make it visible to the WebAssembly host:

```ts
export class Vec2 {
    x: float;
    y: float;
}
```

## Object protocol overrides

Classes can customize how they are printed and hashed by overriding `to_string` and `hash_code`. See [The object type](objects.md) for details.

## Memory

Each class instance is a heap allocation. The memory is freed automatically when the last reference to it drops — no manual `free` needed. If the class defines a `del()` destructor, it runs just before the memory is released. See [Memory Model](../memory.md) for a full explanation.
