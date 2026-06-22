# The `object` Type

`object` is a universal container — it can hold any value: an `int`, a `string`, a class, an array, anything.

## Storing a value

Assigning to an `object` variable automatically boxes the value:

```ts
let o: object = 42;       // boxing an int
let s: object = "hello";  // boxing a string
```

## Reading it back

To get the original value out, cast with the concrete type. If the stored type doesn't match, the program traps at runtime:

```ts
let n = (int)o;    // 42, if o holds an int
```

## The `is` operator

Check the runtime type of an `object` before casting:

```ts
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

On a non-`object` variable, `is` is resolved at compile time. If the types match, the branch is always taken; if they don't, the branch is always skipped (dead code eliminated).

## `to_string` and `hash_code`

Every value responds to `to_string` (returns a `string`) and `hash_code` (returns an `int`):

```ts
let s = to_string(42);       // "42"
let h = hash_code("hello");  // some stable integer
```

These work on any type, including `object`.

## Overriding protocol methods on classes

A class can customize `to_string` and `hash_code` by declaring them with `@override export`:

```ts
class Color {
    r: int;
    g: int;
    b: int;

    @override export fun to_string(): string {
        return "rgb(" + to_string(this.r) + ", " + to_string(this.g) + ", " + to_string(this.b) + ")";
    }

    @override export fun hash_code(): int {
        return this.r * 65536 + this.g * 256 + this.b;
    }
}
```

Requirements:
- Both `@override` and `export` must be present.
- `to_string` must return `string` and take no parameters.
- `hash_code` must return `int` and take no parameters.

Once overridden, calling `print` or `to_string` on a `Color` (or a `Color` stored in an `object`) will use your implementation.
