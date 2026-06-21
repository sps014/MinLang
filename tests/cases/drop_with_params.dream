// A destructor must not declare parameters.
struct Bad {
    id: int;

    pub drop(extra: int) {
        println(this.id);
    }
}

fun main() {
    let b = Bad(1);
    println(b.id);
}
