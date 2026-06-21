enum Color { Red, Green, Blue }
enum Status { Active = 10, Inactive = 20 }

fun describe(c: Color): string {
    switch (c) {
        case Color.Red:
            return "red";
        case Color.Green:
            return "green";
        default:
            return "other";
    }
    return "unreachable";
}

fun main(): void {
    let c: Color = Color.Green;
    println(c);

    print(describe(c));
    print("\n");

    print(describe(Color.Blue));
    print("\n");

    let s: Status = Status.Inactive;
    println(s);

    if (c == Color.Green) {
        println(100);
    }
}
