fun main(): void {
    let i: int = 7;
    let d: double = (double)i;
    print_double(d);

    let f: float = (float)d;
    print_float(f);

    let back: int = (int)d;
    print_int(back);

    let g: double = (double)f;
    print_double(g);
}
