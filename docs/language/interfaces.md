# Interfaces

An **interface** is a contract: a named set of method signatures that a class promises to provide.
A value typed as an interface can hold *any* class that implements it, and method calls on that
value dispatch to the concrete class's implementation at runtime (polymorphism).

## Declaring an interface

An interface lists method signatures — a return type and parameters, but **no body**. Each signature
ends with a semicolon:

```dream
interface Animal {
    fun speak(): string;
    fun legs(): int;
}
```

Interfaces declare methods only. They cannot have fields, and (for now) cannot provide default method
bodies.

## Implementing an interface

A class implements one or more interfaces by listing them after a colon. It must define every method
of each interface with a matching signature:

```dream
class Cat : Animal {
    public fun speak(): string { return "meow"; }
    public fun legs(): int { return 4; }
}

class Dog : Animal {
    public fun speak(): string { return "woof"; }
    public fun legs(): int { return 2; }
}
```

A class can implement several interfaces at once:

```dream
class Robot : Animal, Serializable {
    // ... must implement every method of both Animal and Serializable
}
```

If a class declares `: Animal` but omits one of the interface's methods (or declares it with the
wrong signature), compilation fails with a clear error.

## Using an interface-typed value

A class value is accepted anywhere its interface is expected — this implicit **upcast** needs no
cast. The static type becomes the interface, but the value still remembers its concrete class:

```dream
fun describe(a: Animal): void {
    println(a.speak());   // dispatches to Cat.Speak or Dog.Speak at runtime
    println(a.legs());
}

fun main(): void {
    describe(Cat());      // meow / 4
    describe(Dog());      // woof / 2
}
```

You can also store an interface value explicitly, with or without a cast:

```dream
let c = Cat();
let a: Animal = c;          // implicit upcast
let b = (Animal)c;          // explicit upcast — same value
```

An interface value is just the underlying object, so upcasts and downcasts are free (no copying).

## Interfaces cannot be instantiated

An interface is an abstract contract, not a concrete type — calling it like a constructor is an error:

```dream
let a = Animal();   // error: cannot instantiate interface 'Animal'
```

## Checking the concrete type with `is`

Use `is` to test what an interface value (or an `object`) actually holds at runtime:

```dream
let a: Animal = Cat();
if (a is Cat) {
    println("it's a cat");
}
```

### `is`-with-binding

`is` can bind a new, narrowed local in one step: `expr is Type name` introduces `name: Type` scoped
to the branch guarded by the check, so you don't need a separate cast. It works for **any** target
type:

```dream
let a: Animal = Cat();
if (a is Cat cat) {
    // `cat` is a Cat here, aliasing the same object
    println(cat.speak());
}
```

It also works for value types held in an `object`, unboxing automatically:

```dream
fun show(o: object): void {
    if (o is int n) {
        println(n + 1);   // `n` is an int, unboxed from `o`
    }
}
```

The bound name exists **only** inside the taken branch — it is not visible in the `else` branch or
after the `if`.

!!! note
    `is`-with-binding is supported in `if (...)` conditions. Binding inside `&&` chains or `while`
    conditions is not yet supported.

## How dispatch works

Interface calls use **tag-indexed itables** — the same idea as the JVM's `invokeinterface`. Every
object carries a runtime tag (its concrete class id) in its heap header. For each interface, the
compiler builds a compact table, indexed by that tag, of the concrete method implementations. A call
like `a.speak()` reads the object's tag, looks up the right function in the interface's table, and
calls it indirectly. Because Dream compiles the whole program at once, these tables are computed
entirely at compile time.

## Limits (current version)

- Interfaces declare method signatures only — no fields and no default method bodies.
- Interfaces are non-generic.
- Generic classes cannot implement interfaces.

## See also

- [Classes](classes.md) — defining types, methods, and visibility.
- [The `object` Type](objects.md) — the universal container and the `is` operator.
