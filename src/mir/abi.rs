//! Runtime ABI constants shared between the MIR backend and the embedded runtime `.wat` layers.
//!
//! Every heap block carries a type tag in its header (`[size][tag][ref_count]`). Reference types
//! store their tag in the block they already own; primitives are boxed into a small tagged block.
//! These are the single source of truth for those tags — the `{TAG_*}` placeholders in
//! `runtime/object.wat` / `runtime/format.wat` are substituted from them at emit time, and the host
//! interop layer (`execution/host`) mirrors the same values.

pub const TAG_INT: i32 = 1;
pub const TAG_FLOAT: i32 = 2;
pub const TAG_DOUBLE: i32 = 3;
pub const TAG_BOOL: i32 = 4;
pub const TAG_STRING: i32 = 5;
pub const TAG_ARRAY: i32 = 6;
pub const TAG_CHAR: i32 = 7;
pub const TAG_LONG: i32 = 8;
pub const TAG_UINT: i32 = 9;
pub const TAG_ULONG: i32 = 10;
pub const TAG_BYTE: i32 = 11;
/// Structs/unions are assigned consecutive tags starting here, ordered by sorted type name.
pub const TAG_STRUCT_BASE: i32 = 12;
