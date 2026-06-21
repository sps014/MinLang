fun main(): void {
    let arr: int[] = [10, 20, 30];

    let sum: int = 0;
    for (let x in arr) {
        sum += x;
    }
    print_int(sum);

    let words: string[] = ["a", "b", "c"];
    for (let w in words) {
        print_string(w);
    }
    print_string("\n");

    let count: int = 0;
    for (let y in arr) {
        if (y == 20) { continue; }
        if (y == 30) { break; }
        count += 1;
    }
    print_int(count);

    let total: int = 0;
    for (let a in arr) {
        for (let b in arr) {
            total += 1;
        }
    }
    print_int(total);
}
