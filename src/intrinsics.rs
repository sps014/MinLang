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

/// The attribute that tags a stdlib declaration as a compiler intrinsic, e.g.
/// `@intrinsic("print")`. The parser skips emitting a WASM import for it, the function table
/// records its key, and codegen lowers calls to dedicated runtime helpers. Single source of
/// truth for the attribute name across parser/semantics/codegen.
pub const INTRINSIC_ATTR: &str = "intrinsic";

/// Extracts the intrinsic key from a declaration's attribute list, i.e. the `"name"` in
/// `@intrinsic("name")`, or `None` if the declaration is not an intrinsic. Centralizes the
/// attribute lookup + quote-stripping that was previously duplicated across layers.
pub fn intrinsic_key(
    attributes: &[crate::syntax::nodes::AttributeNode],
) -> Option<String> {
    attributes
        .iter()
        .find(|a| a.name.text == INTRINSIC_ATTR)
        .and_then(|a| a.args.first().map(|arg| arg.text.trim_matches('"').to_string()))
}

/// True if `attributes` contains an `@intrinsic(...)` marker.
pub fn has_intrinsic_attr(attributes: &[crate::syntax::nodes::AttributeNode]) -> bool {
    attributes.iter().any(|a| a.name.text == INTRINSIC_ATTR)
}

// --- Object-protocol builtin free functions -------------------------------------------------
// Callable as `f(x)` with a single argument of any type; lowered to dedicated object-protocol
// runtime helpers rather than to user/stdlib functions.

pub const PRINT: &str = "__print";
pub const PRINTLN: &str = "__println";
pub const TO_STRING: &str = "to_string";
pub const HASH_CODE: &str = "hash_code";

/// The object-protocol builtins.
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

// --- Async intrinsics -----------------------------------------------------------------------

/// `sleep(ms)`: the async timer intrinsic (a free function, unlike the `Promise.*` combinators).
pub const SLEEP: &str = "sleep";

/// Internal free-function names the async combinators lower to (`build_async_intrinsic_call`).
pub const PROMISE_ALL: &str = "__promise_all";
pub const PROMISE_ANY: &str = "__promise_any";
pub const PROMISE_RACE: &str = "__promise_race";

// --- `@intrinsic("…")` static-method registry ----------------------------------------------
// The string inside `@intrinsic("…")` on a stdlib static method. Both semantics (typing) and
// codegen (lowering) classify the attribute key through [`IntrinsicOp`] so the set of attributed
// intrinsics lives in exactly one place instead of being duplicated as bare string `match`es.

pub const ATTR_PRINT: &str = "print";
pub const ATTR_PRINTLN: &str = "println";
pub const ATTR_PROMISE_ALL: &str = "promise_all";
pub const ATTR_PROMISE_ANY: &str = "promise_any";
pub const ATTR_PROMISE_RACE: &str = "promise_race";
pub const ATTR_JSON_SERIALIZE: &str = "json_serialize";
pub const ATTR_JSON_DESERIALIZE: &str = "json_deserialize";

/// The operation a `@intrinsic("…")`-tagged static method lowers to. Derived once from the
/// attribute key via [`IntrinsicOp::from_key`], so every layer dispatches off the same enum
/// rather than re-matching raw strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicOp {
    /// `System.print(x)` — print without trailing newline.
    Print,
    /// `System.println(x)` — print with trailing newline.
    Println,
    /// `Promise.all(xs)` — await all, yield `Future<T[]>`.
    PromiseAll,
    /// `Promise.any(xs)` — first to settle, yield `Future<T>`.
    PromiseAny,
    /// `Promise.race(xs)` — first to settle, yield `Future<T>`.
    PromiseRace,
    /// `JSON.serialize<T>(x)` — `T` to its JSON string.
    JsonSerialize,
    /// `JSON.deserialize<T>(s)` — JSON string to `T`.
    JsonDeserialize,
}

impl IntrinsicOp {
    /// Classifies an `@intrinsic("key")` attribute value, or `None` if `key` is unknown.
    pub fn from_key(key: &str) -> Option<IntrinsicOp> {
        Some(match key {
            ATTR_PRINT => IntrinsicOp::Print,
            ATTR_PRINTLN => IntrinsicOp::Println,
            ATTR_PROMISE_ALL => IntrinsicOp::PromiseAll,
            ATTR_PROMISE_ANY => IntrinsicOp::PromiseAny,
            ATTR_PROMISE_RACE => IntrinsicOp::PromiseRace,
            ATTR_JSON_SERIALIZE => IntrinsicOp::JsonSerialize,
            ATTR_JSON_DESERIALIZE => IntrinsicOp::JsonDeserialize,
            _ => return None,
        })
    }

    /// Classifies the `@intrinsic` attribute on a declaration directly.
    pub fn from_attributes(
        attributes: &[crate::syntax::nodes::AttributeNode],
    ) -> Option<IntrinsicOp> {
        intrinsic_key(attributes).as_deref().and_then(IntrinsicOp::from_key)
    }

    /// For the async combinators, the internal `__promise_*` free-function name they delegate to
    /// (used by both the type checker and codegen); `None` for non-combinator ops.
    pub fn promise_combinator(self) -> Option<&'static str> {
        Some(match self {
            IntrinsicOp::PromiseAll => PROMISE_ALL,
            IntrinsicOp::PromiseAny => PROMISE_ANY,
            IntrinsicOp::PromiseRace => PROMISE_RACE,
            _ => return None,
        })
    }
}
