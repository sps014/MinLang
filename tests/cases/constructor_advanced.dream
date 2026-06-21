// Generic struct with an auto-generated constructor.
struct Box<T> {
    value: T;
}

// Struct holding a reference-typed field, built via the default constructor.
struct Named {
    name: string;
    score: int;
}

// Custom constructor that derives a field, plus a destructor.
struct Account {
    owner: string;
    balance: int;

    pub init(owner: string) {
        this.owner = owner;
        this.balance = 100;
    }

    pub drop() {
        print("closing ");
        println(this.owner);
    }
}

fun open_account(owner: string) {
    let a = Account(owner);
    print(a.owner);
    print(" has ");
    println(a.balance);
}

fun main() {
    let b = Box<int>(42);
    println(b.value);

    let n = Named("Ada", 95);
    print(n.name);
    print(" ");
    println(n.score);

    open_account("Grace");
    open_account("Linus");
    println(0);
}
