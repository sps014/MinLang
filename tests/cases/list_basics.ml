fun main(): void {
    let xs = new_list<int>();
    xs.push(10);
    xs.push(20);
    xs.push(30);
    print_int(xs.size());
    print_int(xs.get(0));
    print_int(xs.get(2));
    xs.set(1, 99);
    print_int(xs.get(1));
    print_int(xs.pop());
    print_int(xs.size());
    print_int(xs.index_of(99));
    print(xs.contains(99));
    print("\n");
    print(xs.contains(12345));
    print("\n");

    let i = 0;
    while (i < 50) {
        xs.push(i);
        i = i + 1;
    }
    print_int(xs.size());
    print_int(xs.get(51));

    xs.remove_at(0);
    print_int(xs.get(0));
    print_int(xs.size());

    let names = new_list<string>();
    names.push("alpha");
    names.push("beta");
    print(names.get(0));
    print("\n");
    print(names.contains("beta"));
    print("\n");
    print_int(names.index_of("beta"));
}
