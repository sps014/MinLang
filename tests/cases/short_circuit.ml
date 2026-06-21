fun side(): bool {
    println(99);
    return true;
}

fun main(): void {
    let f: bool = false;
    if (f && side()) {
        println(1);
    }

    let t: bool = true;
    if (t || side()) {
        println(2);
    }

    println(0);
}
