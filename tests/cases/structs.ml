struct Point {
    x: int;
    y: int;
}

struct Rect {
    p1: Point;
    p2: Point;
}

fun main(): void {
    let p1 = Point { x: 10, y: 20 };
    let p2 = Point { x: 30, y: 40 };
    
    let r = Rect { p1: p1, p2: p2 };
    
    print_int(r.p1.x);
    print_int(r.p1.y);
    print_int(r.p2.x);
    print_int(r.p2.y);
    
    r.p1.x = 100;
    print_int(r.p1.x);
    print_int(p1.x); // Should also be 100 since it's a reference
}