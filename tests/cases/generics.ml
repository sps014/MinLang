fun Test<T>(data: T) {
    if (data is int) {
        print_int(data);
    } else if (data is float) {
        print_float(data);
    } else if (data is string) {
        print_string(data);
    }
}

fun main(): void {
    Test<int>(42);
    Test<float>(3.14f);
    Test<string>("Hello Generics!");
}