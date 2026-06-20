struct Node {
    value: int;
    next: Node;
}

fun create_node(val: int): Node {
    let n = Node { value: val, next: (Node)0 };
    return n;
}

fun main(): void {
    let head = create_node(10);
    head.next = create_node(20);
    head.next.next = create_node(30);
    
    let curr = head;
    while curr != (Node)0 {
        print_int(curr.value);
        curr = curr.next;
    }
}