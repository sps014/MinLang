fun main(): void {
    let missing: string? = null;
    print(missing ?? "fallback");
    print("\n");

    let present: string? = "hello";
    print(present ?? "fallback");
}
