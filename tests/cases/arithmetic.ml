fun main(): void {
    let a = 10;
    let b = 3;
    
    print_int(a + b);
    print_int(a - b);
    print_int(a * b);
    print_int(a / b);
    print_int(a % b);
    
    let x = 1.5;
    let y = 2.0;
    print_float(x + y);
    print_float(x * y);
    
    // Precedence
    print_int(1 + 2 * 3);
    print_int((1 + 2) * 3);
}
