fun main(): void {
    let x = 10;
    
    if (x > 5 ) {
        print_int(1);
    } else {
        print_int(0);
    }
    
    if (x < 5 ) {
        print_int(0);
    } else if (x == 10 ) {
        print_int(2);
    } else {
        print_int(0);
    }
    
    let i = 0;
    while (i < 3 ) {
        print_int(i);
        i = i + 1;
    }
    
    for (let j = 0; j < 3; j = j + 1 ) {
        if (j == 1 ) {
            continue;
        }
        print_int(j);
    }
}
