fun print_greeting(name: string): void {
    let greeting = "Hello, " + name;
    println(greeting);
}

fun main(): void {
    let a = "hello ";
    let b = "world";
    let c = a + b;
    
    println(c);
    
    print_greeting("Alice");
    print_greeting("Bob");
}
