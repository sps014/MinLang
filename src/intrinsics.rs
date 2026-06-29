//! Central registry of compiler *intrinsics*: the builtins and namespaced stdlib operations that
//! the compiler recognizes by name and handles specially - typing them in the semantic analyzer
//! and lowering them in the codegen backend - rather than resolving them through the ordinary
//! function/method tables.
//!
//! Every layer (semantic analysis, codegen, and the codegen-side type inference helper)
//! classifies names through the constants and predicates defined here, so the set of recognized
//! intrinsics - and therefore the surface that has to change when one is added, renamed, or
//! removed - lives in exactly one place. Previously these names were hardcoded as bare string
//! literals duplicated across `type_checker.rs`, `expression.rs`, `statement.rs`, `utils.rs`,
//! `async_support.rs`, and `stdlib/mod.rs`.

// --- Object-protocol builtin free functions -------------------------------------------------
// Callable as `f(x)` with a single argument of any type; lowered to dedicated object-protocol
// runtime helpers rather than to user/stdlib functions.

pub const PRINT: &str = "print";
pub const PRINTLN: &str = "println";
pub const TO_STRING: &str = "to_string";
pub const HASH_CODE: &str = "hash_code";

/// The object-protocol builtins (`print`, `println`, `to_string`, `hash_code`).
pub const OBJECT_BUILTINS: [&str; 4] = [PRINT, PRINTLN, TO_STRING, HASH_CODE];

/// True if `name` is an object-protocol builtin free function.
pub fn is_object_builtin(name: &str) -> bool {
    OBJECT_BUILTINS.contains(&name)
}

/// The generic array-allocation builtin `array_new<T>(len)`.
pub const ARRAY_NEW: &str = "array_new";

// --- Builtin pseudo-methods on language types -----------------------------------------------
// Recognized on built-in types rather than declared as user methods.

/// `string.len()` / `T[].len()`: the length accessor on strings and arrays.
pub const LEN: &str = "len";
/// `<enum>.name()`: the variant-name accessor on enum values.
pub const ENUM_NAME: &str = "name";

// --- Special static namespaces --------------------------------------------------------------
// Receivers that name a compiler-known namespace rather than a value/type in the tables.

pub const MATH: &str = "Math";
pub const PROMISE: &str = "Promise";
pub const JSON: &str = "JSON";

// --- Math namespace -------------------------------------------------------------------------

/// The math functions reachable through `Math.*`, imported from the host. Each takes one numeric
/// argument and returns `float`.
pub const MATH_FUNCTIONS: [&str; 4] = ["sin", "cos", "abs", "sqrt"];

/// True if `name` is a `Math.*` function.
pub fn is_math_function(name: &str) -> bool {
    MATH_FUNCTIONS.contains(&name)
}

// --- Async intrinsics -----------------------------------------------------------------------

/// `sleep(ms)`: the async timer intrinsic (a free function, unlike the `Promise.*` combinators).
pub const SLEEP: &str = "sleep";

pub const PROMISE_ALL: &str = "all";
pub const PROMISE_ANY: &str = "any";
pub const PROMISE_RACE: &str = "race";

/// The async combinators exposed as `Promise.all/any/race`.
pub const PROMISE_COMBINATORS: [&str; 3] = [PROMISE_ALL, PROMISE_ANY, PROMISE_RACE];

/// True if `name` is a `Promise.*` async combinator.
pub fn is_promise_combinator(name: &str) -> bool {
    PROMISE_COMBINATORS.contains(&name)
}

// --- JSON auto-derive intrinsics ------------------------------------------------------------

pub const JSON_SERIALIZE: &str = "serialize";
pub const JSON_SERIALIZE_PRETTY: &str = "serialize_pretty";
pub const JSON_DESERIALIZE: &str = "deserialize";

/// The `JSON.*` auto-derive entry points (`serialize`, `serialize_pretty`, `deserialize`). These
/// have no real signature in `json.dream`; they are backed by the per-class `to_json`/`from_json`
/// converters generated for every `@json` class.
pub const JSON_DERIVE_METHODS: [&str; 3] =
    [JSON_SERIALIZE, JSON_SERIALIZE_PRETTY, JSON_DESERIALIZE];

/// True if `name` is a `JSON.*` auto-derive entry point.
pub fn is_json_derive_method(name: &str) -> bool {
    JSON_DERIVE_METHODS.contains(&name)
}
