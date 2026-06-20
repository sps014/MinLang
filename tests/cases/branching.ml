fun main(): void {
    let x = 10;
    
    // Nested ifs and complex branching
    if (x > 5 ) {
        if (x < 15 ) {
            if (x == 10 ) {
                print_int(1);
            } else {
                print_int(0);
            }
        } else {
            print_int(0);
        }
    } else {
        print_int(0);
    }
    
    // Else if chains
    if (x < 5 ) {
        print_int(0);
    } else if (x == 10 ) {
        print_int(2);
    } else if (x > 10 ) {
        print_int(0);
    } else {
        print_int(0);
    }
    
    // While loop with break
    let i = 0;
    while (i < 10 ) {
        if (i == 3 ) {
            break;
        }
        print_int(i);
        i = i + 1;
    }
    
    // For loop with continue
    for (let j = 0; j < 5; j = j + 1 ) {
        if (j == 1 ) {
            continue;
        }
        if (j == 3 ) {
            continue;
        }
        print_int(j);
    }
    
    // Nested loops
    for (let a = 0; a < 2; a = a + 1 ) {
        for (let b = 0; b < 2; b = b + 1 ) {
            print_int(a * 10 + b);
        }
    }
}
