use crate::syntax::nodes::Type;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use crate::syntax::text::text_span::TextSpan;
use std::rc::Rc;
use crate::syntax::text::line_text::LineText;

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
    ("<std>/regex.dream", include_str!("regex.dream")),
    ("<std>/fetch.dream", include_str!("fetch.dream")),
];

pub struct StdlibFunction {
    pub name: String,
    pub parameters: Vec<String>,
    pub return_type: Option<Type>,
}

impl StdlibFunction {
    fn create_type(type_str: &str) -> Type {
        let dummy_span = TextSpan::new((0, 0), &Rc::new(LineText::new("".to_string())));
        let token = SyntaxToken::new(TokenKind::DataTypeToken, dummy_span, type_str.to_string());
        crate::syntax::nodes::types::primitive_type(type_str, token).unwrap_or(Type::Void)
    }

    /// Host functions that are always imported into every module but are NOT user-callable.
    /// The `print`/`println` builtins lower to these; users never name them directly.
    pub fn host_imports() -> Vec<StdlibFunction> {
        let mut imports = vec![
            StdlibFunction {
                name: "print_string".to_string(),
                parameters: vec!["string".to_string()],
                return_type: None, // void
            },
            StdlibFunction {
                name: "print_int".to_string(),
                parameters: vec!["int".to_string()],
                return_type: None,
            },
            StdlibFunction {
                name: "print_float".to_string(),
                parameters: vec!["float".to_string()],
                return_type: None,
            },
            StdlibFunction {
                name: "print_double".to_string(),
                parameters: vec!["double".to_string()],
                return_type: None,
            },
            StdlibFunction {
                name: "print_char".to_string(),
                parameters: vec!["char".to_string()],
                return_type: None,
            },
        ];
        // Math host functions, reachable only through the `Math.*` namespace. Their names come
        // from the intrinsic registry so the import set never drifts from the recognized set;
        // each takes one `float` and returns a `float`.
        imports.extend(crate::intrinsics::MATH_FUNCTIONS.iter().map(|name| StdlibFunction {
            name: name.to_string(),
            parameters: vec!["float".to_string()],
            return_type: Some(Self::create_type("float")),
        }));
        imports
    }

    /// User-callable stdlib functions registered in the function table. `concat`/`strlen`/
    /// `debug_get_free_list_head` are compiled inline; the math functions are real imports.
    pub fn get_all() -> Vec<StdlibFunction> {
        vec![
            // String
            StdlibFunction {
                name: "concat".to_string(),
                parameters: vec!["string".to_string(), "string".to_string()],
                return_type: Some(Self::create_type("string")),
            },
            StdlibFunction {
                name: "strlen".to_string(),
                parameters: vec!["string".to_string()],
                return_type: Some(Self::create_type("int")),
            },
            // Low-level string/char primitives that the primitive "class" prelude (int/char/string
            // .dream) builds on. Their bodies live in `RUNTIME_STRINGS` (see codegen/wasm/memory.rs).
            StdlibFunction {
                name: "char_at".to_string(),
                parameters: vec!["string".to_string(), "int".to_string()],
                return_type: Some(Self::create_type("char")),
            },
            StdlibFunction {
                name: "string_alloc".to_string(),
                parameters: vec!["int".to_string()],
                return_type: Some(Self::create_type("string")),
            },
            StdlibFunction {
                name: "string_set".to_string(),
                parameters: vec!["string".to_string(), "int".to_string(), "char".to_string()],
                return_type: None,
            },
            StdlibFunction {
                name: "debug_get_free_list_head".to_string(),
                parameters: vec![],
                return_type: Some(Self::create_type("int")),
            },
        ]
    }
}
