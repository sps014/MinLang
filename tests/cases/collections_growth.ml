fun main(): void {
    let xs = new_list<int>();
    let i = 0;
    while (i < 100) {
        xs.push(i * 2);
        i = i + 1;
    }
    print_int(xs.size());
    print_int(xs.get(0));
    print_int(xs.get(99));

    let sum = 0;
    let j = 0;
    while (j < xs.size()) {
        sum = sum + xs.get(j);
        j = j + 1;
    }
    print_int(sum);

    let words = new_list<string>();
    let k = 0;
    while (k < 20) {
        words.push("x");
        k = k + 1;
    }
    print_int(words.size());
    print(words.get(19));
    print("\n");
    print(words.contains("x"));
    print("\n");
    print(words.contains("y"));
    print("\n");

    let squares = new_map<int, int>();
    let n = 0;
    while (n < 64) {
        squares.put(n, n * n);
        n = n + 1;
    }
    print_int(squares.size());
    print_int(squares.get(12));
    print_int(squares.get(63));
}
