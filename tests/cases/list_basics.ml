fun main(): void {
    let xs = List<int>();
    xs.push(10);
    xs.push(20);
    xs.push(30);
    println(xs.size());
    println(xs.get(0));
    println(xs.get(2));
    xs.set(1, 99);
    println(xs.get(1));
    println(xs.pop());
    println(xs.size());
    println(xs.index_of(99));
    print(xs.contains(99));
    print("\n");
    print(xs.contains(12345));
    print("\n");

    let i = 0;
    while (i < 50) {
        xs.push(i);
        i = i + 1;
    }
    println(xs.size());
    println(xs.get(51));

    xs.remove_at(0);
    println(xs.get(0));
    println(xs.size());

    let names = List<string>();
    names.push("alpha");
    names.push("beta");
    print(names.get(0));
    print("\n");
    print(names.contains("beta"));
    print("\n");
    println(names.index_of("beta"));
}
