fun loop_test(): int {
    let a = 0;
    for (let i=0; i<10; i=i+1 ) {
        a = a + i;
    }
    return a;
}

fun string_test(): string {
    let a = "hello ";
    let b = "world";
    let c = a + b;
    return c;
}

fun main(): void {
    print_int(loop_test());
    print(string_test());
    print("\n");
}
