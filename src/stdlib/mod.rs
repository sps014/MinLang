use crate::syntax::nodes::Type;
use crate::syntax::token::syntax_token::SyntaxToken;
use crate::syntax::token::token_kind::TokenKind;
use crate::syntax::text::text_span::TextSpan;
use std::rc::Rc;
use crate::syntax::text::line_text::LineText;

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
        vec![
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
            // Math host functions, reachable only through the `Math.*` namespace.
            StdlibFunction {
                name: "sin".to_string(),
                parameters: vec!["float".to_string()],
                return_type: Some(Self::create_type("float")),
            },
            StdlibFunction {
                name: "cos".to_string(),
                parameters: vec!["float".to_string()],
                return_type: Some(Self::create_type("float")),
            },
            StdlibFunction {
                name: "abs".to_string(),
                parameters: vec!["float".to_string()],
                return_type: Some(Self::create_type("float")),
            },
            StdlibFunction {
                name: "sqrt".to_string(),
                parameters: vec!["float".to_string()],
                return_type: Some(Self::create_type("float")),
            },
        ]
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
            StdlibFunction {
                name: "debug_get_free_list_head".to_string(),
                parameters: vec![],
                return_type: Some(Self::create_type("int")),
            },
        ]
    }
}
