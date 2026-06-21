struct Node {
    val: int;
    next: Node?;
}

fun do_allocations() {
    let n1 = Node { val: 1, next: null };
    let n2 = Node { val: 2, next: n1 };
    let n3 = Node { val: 3, next: n2 };
    // n3 goes out of scope, releasing n2, releasing n1
}

fun main() {
    // Warm up once so the freelist reaches a steady state, then capture the baseline. Genuine
    // reclamation means each subsequent round reuses (and returns) the same blocks, leaving the
    // freelist head unchanged; a leak would keep growing the heap and never return to baseline.
    do_allocations();
    let initial_free = debug_get_free_list_head();

    let i = 0;
    while (i < 100) {
        do_allocations();
        i = i + 1;
    }

    let final_free = debug_get_free_list_head();
    if (initial_free == final_free) {
        print("Memory perfectly reclaimed!");
    } else {
        print("Memory leak detected!");
    }
}