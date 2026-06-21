type Number = int;
type Text = string;

fun add(a: Number, b: Number): Number {
    return a + b;
}

fun main(): void {
    let x: Number = 7;
    let y: Number = 5;
    println(add(x, y));

    let greeting: Text = "hello";
    print(greeting);
}
