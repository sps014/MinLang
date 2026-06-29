use crate::syntax::nodes::Type;
use crate::syntax::text::line_text::LineText;
use crate::syntax::text::text_span::TextSpan;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use std::rc::Rc;

/// The embedded standard-library prelude, in the exact order it must be merged. This is the
/// single source of truth shared by the compiler's source manager and the `dream-analyzer`
/// language service, so the two can never drift. The primitive files (int/char/string/...)
/// only attach methods to built-in types, so their relative order does not matter.
pub const PRELUDE_FILES: &[(&str, &str)] = &[
    ("<std>/list.dream", include_str!("list.dream")),
    ("<std>/map.dream", include_str!("map.dream")),
    ("<std>/int.dream", include_str!("int.dream")),
    ("<std>/char.dream", include_str!("char.dream")),
    ("<std>/string.dream", include_str!("string.dream")),
    ("<std>/bool.dream", include_str!("bool.dream")),
    ("<std>/float.dream", include_str!("float.dream")),
    ("<std>/double.dream", include_str!("double.dream")),
    ("<std>/jsref.dream", include_str!("jsref.dream")),
    ("<std>/json.dream", include_str!("json.dream")),
    ("<std>/math.dream", include_str!("math.dream")),
    ("<std>/regex.dream", include_str!("regex.dream")),
    ("<std>/fetch.dream", include_str!("fetch.dream")),
    ("<std>/system.dream", include_str!("system.dream")),
    ("<std>/promise.dream", include_str!("promise.dream")),
];

pub struct StdlibFunction {
    pub name: String,
    pub parameters: Vec<String>,
    pub return_type: Option<Type>,
    /// When `true`, codegen emits this function's body inline (see `RUNTIME_STRINGS` / the object
    /// runtime) instead of importing it from the host. This is the single source of truth for the
    /// import-vs-inline decision; the module import emitter consults it rather than a parallel list.
    pub inline: bool,
}

impl StdlibFunction {
    fn create_type(type_str: &str) -> Type {
        let dummy_span = TextSpan::new((0, 0), &Rc::new(LineText::new("".to_string())));
        let token = SyntaxToken::new(TokenKind::DataTypeToken, dummy_span, type_str.to_string());
        crate::syntax::nodes::types::primitive_type(type_str, token).unwrap_or(Type::Void)
    }

    /// A host-imported stdlib function (lowered to a WASM `(import "env" ...)`).
    fn imported(name: &str, parameters: &[&str], return_type: Option<Type>) -> Self {
        Self {
            name: name.to_string(),
            parameters: parameters.iter().map(|s| s.to_string()).collect(),
            return_type,
            inline: false,
        }
    }

    /// A stdlib function whose body codegen emits inline rather than importing.
    fn inlined(name: &str, parameters: &[&str], return_type: Option<Type>) -> Self {
        Self {
            name: name.to_string(),
            parameters: parameters.iter().map(|s| s.to_string()).collect(),
            return_type,
            inline: true,
        }
    }

    /// Host functions that are always imported into every module but are NOT user-callable.
    /// The `print`/`println` builtins lower to these; users never name them directly.
    pub fn host_imports() -> Vec<StdlibFunction> {
        let imports = vec![
            Self::imported("print_string", &["string"], None),
            Self::imported("print_int", &["int"], None),
            Self::imported("print_float", &["float"], None),
            Self::imported("print_double", &["double"], None),
            Self::imported("print_char", &["char"], None),
        ];
        imports
    }

    /// User-callable stdlib functions registered in the function table. All of these are compiled
    /// inline (`inline: true`); their bodies live in `RUNTIME_STRINGS` / the object runtime, so the
    /// module emitter skips emitting host imports for them.
    pub fn get_all() -> Vec<StdlibFunction> {
        vec![
            // String
            Self::inlined(
                "concat",
                &["string", "string"],
                Some(Self::create_type("string")),
            ),
            Self::inlined("strlen", &["string"], Some(Self::create_type("int"))),
            // Low-level string/char primitives that the primitive "class" prelude (int/char/string
            // .dream) builds on. Their bodies live in `RUNTIME_STRINGS` (see codegen/wasm/memory.rs).
            Self::inlined(
                "char_at",
                &["string", "int"],
                Some(Self::create_type("char")),
            ),
            Self::inlined("string_alloc", &["int"], Some(Self::create_type("string"))),
            Self::inlined("string_set", &["string", "int", "char"], None),
            Self::inlined(
                "debug_get_free_list_head",
                &[],
                Some(Self::create_type("int")),
            ),
        ]
    }
}
