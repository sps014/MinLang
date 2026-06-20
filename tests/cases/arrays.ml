fun sum_array(arr: int[], len: int): int {
    let sum = 0;
    for let i = 0; i < len; i = i + 1 {
        sum = sum + arr[i];
    }
    return sum;
}

fun modify_array(arr: int[]): void {
    arr[0] = 99;
    arr[1] = 88;
}

fun main(): void {
    let arr1: int[] = [10, 20, 30, 40, 50];
    print_int(arr1[0]);
    print_int(arr1[4]);
    
    // Array passed to function
    print_int(sum_array(arr1, 5));
    
    // Array modification in function
    modify_array(arr1);
    print_int(arr1[0]);
    print_int(arr1[1]);
    print_int(arr1[2]); // Should still be 30
    
    let arr2: float[] = [1.5, 2.5, 3.5];
    print_float(arr2[0]);
    print_float(arr2[2]);
    
    let arr3: string[] = ["hello", "world", "minlang"];
    println(arr3[0]);
    println(arr3[2]);
}
