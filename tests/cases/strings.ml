fun print_greeting(name: string): void {
    print("Hello, ");
    println(name);
}

fun main(): void {
    let a = "hello ";
    let b = "world";
    
    println(a);
    println(b);
    
    print_greeting("Alice");
    print_greeting("Bob");
}
