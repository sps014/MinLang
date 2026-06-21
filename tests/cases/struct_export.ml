struct InternalData {
    value: int;
}

pub struct PublicData {
    val: int;
}

pub fun get_internal(): InternalData {
    return InternalData { value: 1 };
}

pub fun get_public(): PublicData {
    return PublicData { val: 2 };
}

fun main(): void {
    let a = get_public();
}
