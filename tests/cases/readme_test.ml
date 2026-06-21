struct Node {
    value: int;
    next: Node?;

    fun has_next(): bool {
        return this.next != null;
    }
}

fun create_list(n: int): Node? {
    if (n <= 0) {
        return null;
    }
    let head = Node { value: n, next: null };
    let curr: Node? = head;
    let i = n - 1;
    while (i > 0) {
        curr.next = Node { value: i, next: null };
        curr = curr.next;
        i = i - 1;
    }
    return head;
}

fun main(): void {
    let list = create_list(3);
    
    let curr = list;
    while (curr != null) {
        println(curr.value);
        if (curr.has_next()) {
            print(" -> ");
        }
        curr = curr.next;
    }
    print("\n");
}