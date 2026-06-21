fun classify(n: int): string {
    switch (n) {
        case 1, 2, 3:
            return "small";
        case 10:
            return "ten";
        default:
            return "other";
    }
    return "unreachable";
}

fun main(): void {
    print_string(classify(2));
    print_string("\n");
    print_string(classify(10));
    print_string("\n");
    print_string(classify(99));
    print_string("\n");

    let s: string = "hi";
    switch (s) {
        case "hi":
            print_int(1);
        case "bye":
            print_int(2);
        default:
            print_int(0);
    }
}
