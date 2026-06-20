fun fib(n: int): int {
    if (n <= 1 ) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

fun add3(a: int, b: int, c: int): int {
    return a + b + c;
}

fun main(): void {
    print_int(fib(5));
    print_int(fib(7));
    print_int(add3(10, 20, 30));
}
