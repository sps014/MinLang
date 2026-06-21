struct Mixed {
    b1: bool;
    i1: int;
    d1: double;
    b2: bool;
}

fun main(): void {
    let m = Mixed { b1: true, i1: 42, d1: 3.14159d, b2: false };
    
    if (m.b1 ) {
        println(m.i1);
    }
    
    println(m.d1);
    
    m.d1 = 2.718d;
    println(m.d1);
}
