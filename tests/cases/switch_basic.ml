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
    print(classify(2));
    print("\n");
    print(classify(10));
    print("\n");
    print(classify(99));
    print("\n");

    let s: string = "hi";
    switch (s) {
        case "hi":
            println(1);
        case "bye":
            println(2);
        default:
            println(0);
    }
}
