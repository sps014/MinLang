use super::*;
use crate::lang::code_analysis::syntax::lexer::Lexer;
use pretty_assertions::assert_eq;

fn parse_code<'a>(code: &str, arena: &'a bumpalo::Bump) -> (ProgramNode<'a>, DiagnosticBag) {
    let mut diagnostics = DiagnosticBag::new(None);
    let lexer = Lexer::new(code.to_string());
    let mut parser = Parser::new(lexer, arena, &mut diagnostics);
    let tree = parser.parse().unwrap_or_else(|_| crate::lang::code_analysis::syntax::syntax_tree::SyntaxTree::new(ProgramNode::new(vec![], vec![], vec![], vec![])));
    (tree.get_root().clone(), diagnostics)
}

#[test]
fn test_parse_function_declaration() {
    let code = "fun main(): int { return 42; }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);
    
    assert_eq!(diagnostics.has_errors(), false);
    assert_eq!(program.functions.len(), 1);
    
    let func = &program.functions[0];
    assert_eq!(func.name.text, "main");
    assert!(matches!(func.return_type, Some(Type::Integer(_))));
    assert_eq!(func.parameters.len(), 0);
    assert_eq!(func.body.len(), 1);
    
    if let StatementNode::Return(Some(ExpressionNode::Literal(Type::Integer(t)))) = &func.body[0] {
        assert_eq!(t.text, "42");
    } else {
        panic!("Expected return statement with integer literal");
    }
}

#[test]
fn test_parse_array_declaration_and_assignment() {
    let code = "fun test(): void { let arr: int[] = [1, 2, 3]; arr[0] = 5; }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);
    
    assert_eq!(diagnostics.has_errors(), false);
    let func = &program.functions[0];
    assert_eq!(func.body.len(), 2);
    
    // Check declaration
    if let StatementNode::Declaration(id, type_annotation, ExpressionNode::ArrayLiteral(elements), _) = &func.body[0] {
        assert_eq!(id.text, "arr");
        assert!(type_annotation.is_some());
        assert_eq!(elements.len(), 3);
    } else {
        panic!("Expected array declaration");
    }
    
    // Check index assignment
    if let StatementNode::IndexAssignment(arr_expr, index, value) = &func.body[1] {
        if let ExpressionNode::Identifier(id) = *arr_expr {
            assert_eq!(id.text, "arr");
        } else {
            panic!("Expected identifier in index assignment");
        }
        assert!(matches!(**index, ExpressionNode::Literal(Type::Integer(_))));
        assert!(matches!(value, ExpressionNode::Literal(Type::Integer(_))));
    } else {
        panic!("Expected index assignment");
    }
}

#[test]
fn test_parse_binary_expression_precedence() {
    let code = "fun test(): void { let x = 1 + 2 * 3; }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);
    
    assert_eq!(diagnostics.has_errors(), false);
    let func = &program.functions[0];
    
    if let StatementNode::Declaration(_, _, ExpressionNode::Binary(left, opr, right), _) = &func.body[0] {
        assert_eq!(opr.kind, TokenKind::PlusToken);
        assert!(matches!(**left, ExpressionNode::Literal(Type::Integer(_))));
        assert!(matches!(**right, ExpressionNode::Binary(_, _, _))); // The * should be grouped on the right
    } else {
        panic!("Expected binary expression with correct precedence");
    }
}

#[test]
fn test_parse_extern_function() {
    let code = "extern fun alert(msg: string): void;";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    assert_eq!(program.functions.len(), 1);

    let func = &program.functions[0];
    assert_eq!(func.name.text, "alert");
    assert!(func.is_extern);
    assert_eq!(func.body.len(), 0);
    assert_eq!(func.parameters.len(), 1);
    // Defaults: import module "env", import name = function name.
    assert_eq!(func.import_module.as_deref(), Some("env"));
    assert_eq!(func.import_name.as_deref(), Some("alert"));
}

#[test]
fn test_parse_extern_with_js_attribute() {
    let code = "@js(\"dom\", \"setText\") extern fun set_text(v: string): int;";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    let func = &program.functions[0];
    assert!(func.is_extern);
    assert_eq!(func.import_module.as_deref(), Some("dom"));
    assert_eq!(func.import_name.as_deref(), Some("setText"));
}

#[test]
fn test_parse_extern_rejects_body() {
    let code = "extern fun bad(): void { return; }";
    let arena = bumpalo::Bump::new();
    let (_, diagnostics) = parse_code(code, &arena);

    // A body where a `;` is expected must produce a diagnostic.
    assert_eq!(diagnostics.has_errors(), true);
}

#[test]
fn test_parse_enum_declaration() {
    let code = "enum Color { Red, Green = 5, Blue }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    assert_eq!(program.enums.len(), 1);

    let decl = &program.enums[0];
    assert_eq!(decl.name.text, "Color");
    assert_eq!(decl.members.len(), 3);
    // Auto-assigned, explicit, then continues from explicit value.
    assert_eq!(decl.members[0].0.text, "Red");
    assert_eq!(decl.members[0].1, 0);
    assert_eq!(decl.members[1].0.text, "Green");
    assert_eq!(decl.members[1].1, 5);
    assert_eq!(decl.members[2].0.text, "Blue");
    assert_eq!(decl.members[2].1, 6);
}

#[test]
fn test_parse_do_while() {
    let code = "fun test(): void { do { print_int(1); } while (false); }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    let func = &program.functions[0];
    assert!(matches!(func.body[0], StatementNode::DoWhile(_, _)));
}

#[test]
fn test_parse_const_and_labeled_break() {
    let code = "fun test(): void { const x: int = 1; loop: while (true) { break loop; } }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    let func = &program.functions[0];
    // First statement is a const declaration (is_const == true).
    assert!(matches!(&func.body[0], StatementNode::Declaration(_, _, _, true)));
    // Second statement is a labeled loop containing a `break loop;`.
    if let StatementNode::Labeled(label, inner) = &func.body[1] {
        assert_eq!(label, "loop");
        if let StatementNode::While(_, body) = inner {
            assert!(matches!(&body[0], StatementNode::Break(Some(l)) if l == "loop"));
        } else {
            panic!("Expected labeled while loop");
        }
    } else {
        panic!("Expected labeled statement");
    }
}

#[test]
fn test_parse_char_literal() {
    let code = "fun test(): void { let c: int = 'A'; }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    let func = &program.functions[0];
    if let StatementNode::Declaration(_, _, ExpressionNode::Literal(Type::Integer(t)), _) = &func.body[0] {
        assert_eq!(t.text, "65");
    } else {
        panic!("Expected char literal lowered to integer 65");
    }
}

#[test]
fn test_parse_error_recovery() {
    let code = "fun test(): void { let x = ; let y = 5; }";
    let arena = bumpalo::Bump::new();
    let (_, diagnostics) = parse_code(code, &arena);
    
    assert_eq!(diagnostics.has_errors(), true);
    // The parser should report an error for the missing expression but continue parsing `let y = 5;`
    assert!(diagnostics.diagnostics.len() > 0);
}
