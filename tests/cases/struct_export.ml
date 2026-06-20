struct InternalData {
    value: int;
}

export struct PublicData {
    val: int;
}

export fun get_internal(): InternalData {
    return InternalData { value: 1 };
}

export fun get_public(): PublicData {
    return PublicData { val: 2 };
}

fun main(): void {
    let a = get_public();
}
