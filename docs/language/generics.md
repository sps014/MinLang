# Generics

Generics let you write code that works for any type without duplicating it. MinLang resolves generics at compile time — the compiler produces a separate, fully optimized copy of your code for each concrete type you use.

## Generic functions

Add `<T>` after the function name:

```minlang
fun first<T>(arr: T[]): T {
    return arr[0];
}

fun main(): void {
    let nums = [10, 20, 30];
    let words = ["a", "b", "c"];
    print(first<int>(nums));     // 10
    print(first<string>(words)); // a
    print("\n");
}
```

The type argument can often be inferred from the call site, though explicit `<Type>` is always accepted.

Multiple type parameters:

```minlang
fun swap<A, B>(a: A, b: B): A {
    return a;
}
```

## Generic structs

Structs can be generic too:

```minlang
struct Pair<A, B> {
    first: A;
    second: B;
}

fun main(): void {
    let p = Pair<int, string> { first: 1, second: "one" };
    print(p.first);
    print("\n");
    print(p.second);
    print("\n");
}
```

## Generic methods

Methods on generic structs automatically have access to the struct's type parameters:

```minlang
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
    print(b.get());   // 100
}
```

## Type checking inside generic bodies

Use `is` to branch on the concrete type at compile time. The compiler eliminates dead branches entirely:

```minlang
fun describe<T>(v: T): void {
    if (v is int) {
        print("it's an int: ");
        print(v);
        print("\n");
    } else if (v is string) {
        print("it's a string: ");
        print(v);
        print("\n");
    }
}
```

## How it works

Every unique combination of type arguments creates a new instantiation. `Box<int>` and `Box<string>` are entirely separate types in the compiled output. There is no boxing, no virtual dispatch, and no runtime overhead compared to writing the type-specific code by hand.
