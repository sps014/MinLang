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
    print_int(c);

    print_string(describe(c));
    print_string("\n");

    print_string(describe(Color.Blue));
    print_string("\n");

    let s: Status = Status.Inactive;
    print_int(s);

    if (c == Color.Green) {
        print_int(100);
    }
}
