use crate::lang::code_analysis::syntax::nodes::Type;
use crate::lang::code_analysis::token::syntax_token::SyntaxToken;
use crate::lang::code_analysis::token::token_kind::TokenKind;
use crate::lang::code_analysis::text::text_span::TextSpan;
use std::rc::Rc;
use crate::lang::code_analysis::text::line_text::LineText;

pub struct StdlibFunction {
    pub name: String,
    pub parameters: Vec<String>,
    pub return_type: Option<Type>,
}

impl StdlibFunction {
    fn create_type(type_str: &str) -> Type {
        let dummy_span = TextSpan::new((0, 0), &Rc::new(LineText::new("".to_string())));
        let token = SyntaxToken::new(TokenKind::DataTypeToken, dummy_span, type_str.to_string());
        match type_str {
            "int" => Type::Integer(token),
            "float" => Type::Float(token),
            "string" => Type::String(token),
            "bool" => Type::Boolean(token),
            _ => Type::Void,
        }
    }

    pub fn get_all() -> Vec<StdlibFunction> {
        vec![
            // I/O
            StdlibFunction {
                name: "print".to_string(),
                parameters: vec!["string".to_string()],
                return_type: None, // void
            },
            StdlibFunction {
                name: "println".to_string(),
                parameters: vec!["string".to_string()],
                return_type: None,
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
            
            // Math
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
            
            // Memory
            StdlibFunction {
                name: "malloc".to_string(),
                parameters: vec!["int".to_string()],
                return_type: Some(Self::create_type("int")), // pointer
            },
            StdlibFunction {
                name: "free".to_string(),
                parameters: vec!["int".to_string()],
                return_type: None,
            },
        ]
    }
}
