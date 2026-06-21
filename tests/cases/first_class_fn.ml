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
    println(apply(twice, 5));
    println(apply(thrice, 5));

    let arr: int[] = [1, 2, 3];
    println(map_sum(arr, twice));
    println(map_sum(arr, thrice));

    let g: fun(int): int = twice;
    println(g(7));
}
