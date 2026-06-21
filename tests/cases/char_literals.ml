fun main(): void {
    let a: char = 'A';
    let z: char = 'z';
    print(a);
    print(z);
    print('\n');

    let code: int = (int)a;
    print(to_string(code));
    print('\n');

    let next: char = (char)(code + 1);
    print(next);
    print('\n');

    let chars: char[] = ['h', 'i'];
    print(chars[0]);
    print(chars[1]);
    print('\n');

    if (a < z) {
        print(to_string(1));
    }
    print('\n');
}
