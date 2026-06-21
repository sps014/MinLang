fun twice(x: int): int {
    return x * 2;
}

fun thrice(x: int): int {
    return x * 3;
}

fun apply(f: fun(int): int, v: int): int {
    return f(v);
}

fun map_sum(arr: int[], f: fun(int): int): int {
    let sum: int = 0;
    for (let x in arr) {
        sum += f(x);
    }
    return sum;
}

fun main(): void {
    print_int(apply(twice, 5));
    print_int(apply(thrice, 5));

    let arr: int[] = [1, 2, 3];
    print_int(map_sum(arr, twice));
    print_int(map_sum(arr, thrice));

    let g: fun(int): int = twice;
    print_int(g(7));
}
