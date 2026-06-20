// A struct method that shadows an object-protocol method must be marked `@override`.
struct Foo {
    x: int;

    export fun to_string(): string {
        return "foo";
    }
}

fun main() {
    let f = Foo { x: 1 };
    print(to_string(f));
}
