fun main(): void {
    let m = Map<string, int>();
    m.put("one", 1);
    m.put("two", 2);
    m.put("three", 3);
    println(m.size());
    println(m.get("one"));
    println(m.get("three"));
    print(m.contains("two"));
    print("\n");
    print(m.contains("nope"));
    print("\n");
    println(m.get("missing"));
    println(m.get_or("missing", -1));
    m.put("two", 22);
    println(m.get("two"));
    println(m.size());
    print(m.remove("two"));
    print("\n");
    println(m.size());
    print(m.contains("two"));
    print("\n");
    println(m.get("two"));

    let counts = Map<int, int>();
    let i = 0;
    while (i < 40) {
        counts.put(i, i * i);
        i = i + 1;
    }
    println(counts.size());
    println(counts.get(7));
    println(counts.get(39));
    print(counts.contains(40));
    print("\n");
    println(counts.get_or(40, -7));
}
