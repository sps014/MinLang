// Object type: boxing primitives, to_string, unboxing via cast, runtime `is`,
// and reference (pointer) equality.
fun main() {
    let o: object = 42;
    print(to_string(o));
    print("\n");

    let n: int = (int)o;
    print(n);

    let s: object = "hi";
    print(to_string(s));
    print("\n");

    let b: object = true;
    print(to_string(b));
    print("\n");

    if (o is int) { print("o is int\n"); }
    if (s is string) { print("s is string\n"); }
    if (o is string) { print("o is string\n"); } else { print("o not string\n"); }

    let a: object = "x";
    let c: object = a;
    if (a == c) { print("same\n"); }

    print(to_string(hash_code(7)));
    print("\n");
}
