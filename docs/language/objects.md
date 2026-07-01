# The `object` Type

`object` is a universal container — it can hold any value: an `int`, a `string`, a class, an array, anything.

## Storing a value

Assigning to an `object` variable automatically boxes the value:

```dream
let o: object = 42;       // boxing an int
let s: object = "hello";  // boxing a string
```

## Reading it back

To get the original value out, cast with the concrete type. If the stored type doesn't match, the program traps at runtime:

```dream
let n = (int)o;    // 42, if o holds an int
```

## The `is` operator

Check the runtime type of an `object` before casting:

```dream
fun describe(o: object): void {
    if (o is int) {
        print("int: ");
        println((int)o);
    } else if (o is string) {
        print("string: ");
        println((string)o);
    } else {
        println("something else");
    }
}
```

On a non-`object` variable, `is` is resolved at compile time. If the types match, the branch is always taken; if they don't, the branch is always skipped (dead code eliminated). `is` also works on [interface](interfaces.md)-typed values, checking the concrete class at runtime.

## `is`-with-binding

`is` can declare a narrowed local at the same time — `expr is Type name` binds `name: Type` inside the branch, so no separate cast is needed:

```dream
fun describe(o: object): void {
    if (o is int n) {
        println(n + 1);          // `n` is an int, unboxed from `o`
    } else if (o is string s) {
        println(s);              // `s` is a string
    }
}
```

The bound name is visible only inside the taken branch. It works for any target type — primitives (unboxed) and reference/interface types (aliased). See [Interfaces](interfaces.md#is-with-binding) for more.

## `to_string` and `hash_code`

Every value responds to the instance methods `to_string()` (returns a `string`) and `hash_code()`
(returns an `int`):

```dream
let s = (42).to_string();       // "42"
let h = "hello".hash_code();    // some stable integer
```

These work on any type, including `object`.

## Overriding protocol methods on classes

A class can customize `to_string` and `hash_code` by declaring them with `@override public`:

```dream
class Color {
    r: int;
    g: int;
    b: int;

    @override public fun to_string(): string {
        return "rgb(" + this.r + ", " + this.g + ", " + this.b + ")";
    }

    @override public fun hash_code(): int {
        return this.r * 65536 + this.g * 256 + this.b;
    }
}
```

Requirements:
- Both `@override` and `public` must be present.
- `to_string` must return `string` and take no parameters.
- `hash_code` must return `int` and take no parameters.

Once overridden, calling `print` or `to_string` on a `Color` (or a `Color` stored in an `object`) will use your implementation.
