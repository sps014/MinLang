// Auto-generated (default) constructor: Point(x, y) stores fields positionally.
struct Point {
    x: int;
    y: int;
}

// Custom constructor via `pub init`, plus a destructor via `pub drop`.
struct Logged {
    id: int;

    pub init(id: int) {
        this.id = id;
        print("create ");
        println(this.id);
    }

    pub drop() {
        print("drop ");
        println(this.id);
    }
}

fun make(n: int) {
    let temp = Logged(n);
    println(temp.id);
}

fun main() {
    let p = Point(3, 4);
    println(p.x);
    println(p.y);

    make(7);
    println(999);
}
