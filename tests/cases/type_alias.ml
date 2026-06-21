type Number = int;
type Text = string;

fun add(a: Number, b: Number): Number {
    return a + b;
}

fun main(): void {
    let x: Number = 7;
    let y: Number = 5;
    print_int(add(x, y));

    let greeting: Text = "hello";
    print_string(greeting);
}
