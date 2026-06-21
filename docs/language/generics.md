# Generics

Generics let you write code that works for any type without duplicating it. Dream resolves generics at compile time — the compiler produces a separate, fully optimized copy of your code for each concrete type you use.

## Generic functions

Add `<T>` after the function name:

```kotlin
fun first<T>(arr: T[]): T {
    return arr[0];
}

fun main(): void {
    let nums = [10, 20, 30];
    let words = ["a", "b", "c"];
    println(first<int>(nums));     // 10
    println(first<string>(words)); // a
}
```

The type argument can often be inferred from the call site, though explicit `<Type>` is always accepted.

Multiple type parameters:

```kotlin
fun swap<A, B>(a: A, b: B): A {
    return a;
}
```

## Generic structs

Structs can be generic too:

```kotlin
struct Pair<A, B> {
    first: A;
    second: B;
}

fun main(): void {
    let p = Pair<int, string> { first: 1, second: "one" };
    println(p.first);
    println(p.second);
}
```

Type arguments can themselves be generic (or arrays), so generics nest freely:

```kotlin
let nested = Pair<Box<int>, int> { first: Box<int> { v: 7 }, second: 5 };
println(nested.first.v);   // 7

let pts: Pair<int, int>[] = [Pair<int, int> { first: 1, second: 2 }];
println(pts[0].second);    // 2
```

## Generic methods

Methods on generic structs automatically have access to the struct's type parameters:

```kotlin
struct Box<T> {
    value: T;

    fun get(): T {
        return this.value;
    }

    fun set(v: T): void {
        this.value = v;
    }
}

fun main(): void {
    let b = Box<int> { value: 42 };
    b.set(100);
    println(b.get());   // 100
}
```

## Type checking inside generic bodies

Use `is` to branch on the concrete type at compile time. The compiler eliminates dead branches entirely:

```kotlin
fun describe<T>(v: T): void {
    if (v is int) {
        print("it's an int: ");
        println(v);
    } else if (v is string) {
        print("it's a string: ");
        println(v);
    }
}
```

## How it works

Every unique combination of type arguments creates a new instantiation. `Box<int>` and `Box<string>` are entirely separate types in the compiled output. There is no boxing, no virtual dispatch, and no runtime overhead compared to writing the type-specific code by hand.
