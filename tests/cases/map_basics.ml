fun main(): void {
    let m = new_map<string, int>();
    m.put("one", 1);
    m.put("two", 2);
    m.put("three", 3);
    print_int(m.size());
    print_int(m.get("one"));
    print_int(m.get("three"));
    print(m.contains("two"));
    print("\n");
    print(m.contains("nope"));
    print("\n");
    print_int(m.get("missing"));
    print_int(m.get_or("missing", -1));
    m.put("two", 22);
    print_int(m.get("two"));
    print_int(m.size());
    print(m.remove("two"));
    print("\n");
    print_int(m.size());
    print(m.contains("two"));
    print("\n");
    print_int(m.get("two"));

    let counts = new_map<int, int>();
    let i = 0;
    while (i < 40) {
        counts.put(i, i * i);
        i = i + 1;
    }
    print_int(counts.size());
    print_int(counts.get(7));
    print_int(counts.get(39));
    print(counts.contains(40));
    print("\n");
    print_int(counts.get_or(40, -7));
}
