fun main(): void {
    let x: int = 5;
    let y: int = x > 3 ? 100 : 200;
    println(y);

    let s: string = x > 10 ? "big" : "small";
    print(s);
    print("\n");

    let z: int = x > 10 ? 1 : x > 3 ? 2 : 3;
    println(z);
}
