struct Box<T> {
    v: T;
}

struct Pair<A, B> {
    first: A;
    second: B;
}

fun main() {
    let b = Box<Box<int>> { v: Box<int> { v: 7 } };
    println(b.v.v);

    let p = Pair<Box<int>, int> { first: Box<int> { v: 99 }, second: 5 };
    println(p.first.v);
    println(p.second);
}
