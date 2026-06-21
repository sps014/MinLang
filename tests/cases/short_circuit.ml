fun side(): bool {
    print_int(99);
    return true;
}

fun main(): void {
    let f: bool = false;
    if (f && side()) {
        print_int(1);
    }

    let t: bool = true;
    if (t || side()) {
        print_int(2);
    }

    print_int(0);
}
