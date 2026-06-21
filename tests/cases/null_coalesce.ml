fun main(): void {
    let missing: string? = null;
    print_string(missing ?? "fallback");
    print_string("\n");

    let present: string? = "hello";
    print_string(present ?? "fallback");
}
