// Multiple generic parameters: two-parameter struct (with a method using both params),
// two-parameter functions, and a mangling-collision check between Pair<int, float> and
// Pair<int, double> (which previously both collapsed to "Pair_int").
struct Pair<K, V> {
    key: K;
    value: V;

    fun show() {
        print(this.key);
        print(this.value);
    }
}

fun first<A, B>(a: A, b: B): A {
    return a;
}

fun second<A, B>(a: A, b: B): B {
    return b;
}

fun main() {
    let p = Pair<int, string> { key: 1, value: "one\n" };
    p.show();

    let q = Pair<int, float> { key: 2, value: 2.5f };
    print(q.value);

    let r = Pair<int, double> { key: 3, value: 3.5d };
    print(r.value);

    print(first<int, string>(10, "ten"));
    let s = second<int, string>(10, "two\n");
    print(s);
}
