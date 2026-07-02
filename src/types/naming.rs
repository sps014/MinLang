//! Backend naming and inline-layout helpers.
//!
//! These derive the internal symbol names the semantic analyzer and codegen agree on (method,
//! constructor, and `@json` converter names) and the byte size/alignment a scalar occupies when
//! stored inline. They are backend concerns, kept out of the `dream-syntax` surface AST so the
//! frontend carries no mangling/layout knowledge.

use crate::syntax::nodes::types::CONSTRUCTOR_NAME;

/// The internal name under which a struct method is registered in the function table and emitted in
/// codegen: the struct name and method name joined with `_` (e.g. `User_greet`). Single source of
/// truth for method-name mangling; the derived-method helpers below build on it.
pub fn method_fn(struct_name: &str, method_name: &str) -> String {
    format!("{}_{}", struct_name, method_name)
}

/// The internal name under which a struct's user-defined constructor is registered/emitted
/// (e.g. `User_constructor`). Single source of truth for the constructor naming convention.
pub fn constructor_fn(struct_name: &str) -> String {
    method_fn(struct_name, CONSTRUCTOR_NAME)
}

/// The name of the compiler-derived `to_json` converter for a `@json` struct (e.g. `User_to_json`).
/// Single source of truth for the implicit naming contract shared by the `@json` source generator,
/// the type checker, and the codegen backend.
pub fn json_to_json_fn(struct_name: &str) -> String {
    method_fn(struct_name, "to_json")
}

/// The name of the compiler-derived `from_json` converter for a `@json` struct (e.g.
/// `User_from_json`). See [`json_to_json_fn`].
pub fn json_from_json_fn(struct_name: &str) -> String {
    method_fn(struct_name, "from_json")
}

/// Byte size and alignment of a value of `type_name` when stored inline (array element or struct
/// field). `bool`/`char`/`byte` occupy a single byte; `double`/`long`/`ulong` are 8 bytes;
/// everything else - `int`, `uint`, `float`, and all heap references - is a 4-byte word/pointer.
pub fn value_size_align(type_name: &str) -> (usize, usize) {
    match type_name {
        "bool" | "char" | "byte" => (1, 1),
        "double" | "long" | "ulong" => (8, 8),
        _ => (4, 4),
    }
}
