fun main(): void {
    let xs = List<int>();
    let i = 0;
    while (i < 100) {
        xs.push(i * 2);
        i = i + 1;
    }
    print(xs.size());
    print(xs.get(0));
    print(xs.get(99));

    let sum = 0;
    let j = 0;
    while (j < xs.size()) {
        sum = sum + xs.get(j);
        j = j + 1;
    }
    print(sum);

    let words = List<string>();
    let k = 0;
    while (k < 20) {
        words.push("x");
        k = k + 1;
    }
    print(words.size());
    print(words.get(19));
    print("\n");
    print(words.contains("x"));
    print("\n");
    print(words.contains("y"));
    print("\n");

    let squares = Map<int, int>();
    let n = 0;
    while (n < 64) {
        squares.put(n, n * n);
        n = n + 1;
    }
    print(squares.size());
    print(squares.get(12));
    print(squares.get(63));
}
