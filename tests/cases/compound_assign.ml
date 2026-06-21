struct P { v: int; }

fun main(): void {
    let x: int = 10;
    x += 5;
    print_int(x);
    x -= 3;
    print_int(x);
    x *= 2;
    print_int(x);
    x /= 4;
    print_int(x);
    x %= 4;
    print_int(x);
    x++;
    print_int(x);
    x--;
    print_int(x);

    let arr: int[] = [1, 2, 3];
    arr[1] += 10;
    print_int(arr[1]);

    let p = P { v: 5 };
    p.v += 7;
    print_int(p.v);
}
