fun main(): void {
    let m = Map<string, int>();
    m.put("one", 1);
    m.put("two", 2);
    m.put("three", 3);
    print(m.size());
    print(m.get("one"));
    print(m.get("three"));
    print(m.contains("two"));
    print("\n");
    print(m.contains("nope"));
    print("\n");
    print(m.get("missing"));
    print(m.get_or("missing", -1));
    m.put("two", 22);
    print(m.get("two"));
    print(m.size());
    print(m.remove("two"));
    print("\n");
    print(m.size());
    print(m.contains("two"));
    print("\n");
    print(m.get("two"));

    let counts = Map<int, int>();
    let i = 0;
    while (i < 40) {
        counts.put(i, i * i);
        i = i + 1;
    }
    print(counts.size());
    print(counts.get(7));
    print(counts.get(39));
    print(counts.contains(40));
    print("\n");
    print(counts.get_or(40, -7));
}
