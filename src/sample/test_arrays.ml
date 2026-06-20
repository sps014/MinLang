fun main(): void {
    let arr: int[] = [10, 20, 30];
    print_int(arr[0]);
    print_int(arr[1]);
    print_int(arr[2]);
    
    arr[1] = 50;
    print_int(arr[1]);
    
    let arr2: float[] = [1.5, 2.5];
    print_float(arr2[0]);
    print_float(arr2[1]);
    
    let arr3: string[] = ["hello", "world"];
    println(arr3[0]);
    println(arr3[1]);
}
