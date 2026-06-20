// The object protocol: compile-time generated default to_string/hash_code for a struct,
// and @override export implementations for another struct, plus runtime dispatch through
// an `object`.
struct Point {
    x: int;
    y: int;
}

struct Named {
    id: int;
    label: string;

    @override export fun to_string(): string {
        return "Named#" + to_string(this.id);
    }

    @override export fun hash_code(): int {
        return this.id;
    }
}

fun main() {
    let p = Point { x: 3, y: 7 };
    print(to_string(p));
    print("\n");
    print(hash_code(p));

    let n = Named { id: 5, label: "hello" };
    print(to_string(n));
    print("\n");
    print(hash_code(n));

    let o: object = p;
    print(to_string(o));
    print("\n");
}
