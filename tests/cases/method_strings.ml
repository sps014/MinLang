// Regression test: string literals inside method bodies and passed as method
// arguments must be collected into the data segment (previously panicked at codegen).
struct Greeter {
    name: string;

    fun greet() {
        print("hello from method body\n");
    }

    fun say(msg: string) {
        print(msg);
    }
}

fun main() {
    let g = Greeter { name: "ignored" };
    g.greet();
    g.say("string passed to method\n");
}
