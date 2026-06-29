use super::*;
use crate::syntax::lexer::Lexer;
use crate::syntax::nodes::{ExpressionNode, StatementNode};
use pretty_assertions::assert_eq;

fn parse_code<'a>(code: &str, arena: &'a bumpalo::Bump) -> (ProgramNode<'a>, DiagnosticBag) {
    let mut diagnostics = DiagnosticBag::new(None);
    let lexer = Lexer::new(code.to_string());
    let mut parser = Parser::new(lexer, arena, &mut diagnostics);
    let tree = parser.parse().unwrap_or_else(|_| {
        crate::syntax::syntax_tree::SyntaxTree::new(ProgramNode::new(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ))
    });
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
    if let StatementNode::Declaration(
        id,
        type_annotation,
        ExpressionNode::ArrayLiteral(elements),
        _,
    ) = &func.body[0]
    {
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

    if let StatementNode::Declaration(_, _, ExpressionNode::Binary(left, opr, right), _) =
        &func.body[0]
    {
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
    let js_attr = func.attributes.iter().find(|a| a.name.text == "js");
    assert!(js_attr.is_none());
}

#[test]
fn test_parse_extern_with_js_attribute() {
    let code = "@js(\"dom\", \"setText\") extern fun set_text(v: string): int;";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    let func = &program.functions[0];
    assert!(func.is_extern);
    let js_attr = func
        .attributes
        .iter()
        .find(|a| a.name.text == "js")
        .unwrap();
    assert_eq!(js_attr.args.get(0).unwrap().text, "\"dom\"");
    assert_eq!(js_attr.args.get(1).unwrap().text, "\"setText\"");
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
    assert!(matches!(
        &func.body[0],
        StatementNode::Declaration(_, _, _, true)
    ));
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
    let code = "fun test(): void { let c: char = 'A'; }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    let func = &program.functions[0];
    if let StatementNode::Declaration(_, _, ExpressionNode::Literal(Type::Char(t)), _) =
        &func.body[0]
    {
        assert_eq!(t.text, "65");
    } else {
        panic!("Expected char literal with code point 65");
    }
}

#[test]
fn test_parse_error_recovery() {
    let code = "fun test(): void { let x = ; let y = 5; }";
    let arena = bumpalo::Bump::new();
    let (_, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), true);
    // The parser should report an error for the missing expression but continue parsing `let y = 5;`
    assert!(!diagnostics.diagnostics.is_empty());
}

#[test]
fn test_parse_nested_generic_type_annotation() {
    // Nested generics close with `>>` (a single ShiftRight token); the parser must split it.
    let code = "fun main(): void { let b: Box<Box<int>> = null; }";
    let arena = bumpalo::Bump::new();
    let (_, diagnostics) = parse_code(code, &arena);
    assert_eq!(diagnostics.has_errors(), false);
}

#[test]
fn test_parse_multi_arg_nested_generic_instantiation() {
    // `Pair<Box<int>, int>(...)` must be recognized as a (constructor) call despite the
    // nested generic in the first type argument.
    let code = "class Box<T> { v: T; } class Pair<A, B> { first: A; second: B; } \
                fun main(): void { let p = Pair<Box<int>, int>(Box<int>(1), 2); }";
    let arena = bumpalo::Bump::new();
    let (_, diagnostics) = parse_code(code, &arena);
    assert_eq!(diagnostics.has_errors(), false);
}

#[test]
fn test_parse_struct_comma_fields_recovers_without_hanging() {
    // Comma-separated class fields are invalid (fields use ';'). The parser must report an
    // error and terminate rather than spin forever on the unexpected token.
    let code = "class Point { x: int, y: int, } fun main(): void { }";
    let arena = bumpalo::Bump::new();
    let (_, diagnostics) = parse_code(code, &arena);
    assert_eq!(diagnostics.has_errors(), true);
}

#[test]
fn test_parse_struct_constructor_and_destructor() {
    // `constructor(...)` and `del()` parse as class methods named `constructor` / `del`
    // without the `fun` keyword or a return type.
    let code = "class Point { x: int; y: int; \
                constructor(x: int, y: int) { this.x = x; this.y = y; } \
                del() { } \
                fun sum(): int { return this.x + this.y; } }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    assert_eq!(program.structs.len(), 1);
    let s = &program.structs[0];
    assert_eq!(s.fields.len(), 2);

    let init = s
        .methods
        .iter()
        .find(|m| m.name.text == "constructor")
        .expect("constructor method");
    assert_eq!(init.parameters.len(), 2);
    assert!(init.return_type.is_none());

    let drop = s
        .methods
        .iter()
        .find(|m| m.name.text == "del")
        .expect("del method");
    assert_eq!(drop.parameters.len(), 0);
    assert!(drop.return_type.is_none());

    assert!(s.methods.iter().any(|m| m.name.text == "sum"));
}

#[test]
fn test_parse_async_function_and_await() {
    // `async fun` sets `is_async`; `await e;` is an `AwaitStmt` and `let x = await e;` carries an
    // `Await` initializer.
    let code = "async fun f(): int { await sleep(1); let x = await f(); return x; }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);

    assert_eq!(diagnostics.has_errors(), false);
    let func = &program.functions[0];
    assert!(func.is_async);
    assert!(matches!(&func.body[0], StatementNode::AwaitStmt(_)));
    assert!(matches!(
        &func.body[1],
        StatementNode::Declaration(_, _, ExpressionNode::Await(_), _)
    ));
}

#[test]
fn test_parse_extern_async_either_order() {
    // Both `extern async fun` and `async extern fun` parse to an async extern import.
    for code in [
        "extern async fun g(id: int): string;",
        "async extern fun g(id: int): string;",
    ] {
        let arena = bumpalo::Bump::new();
        let (program, diagnostics) = parse_code(code, &arena);
        assert_eq!(diagnostics.has_errors(), false, "code: {}", code);
        let func = &program.functions[0];
        assert!(func.is_extern, "code: {}", code);
        assert!(func.is_async, "code: {}", code);
    }
}

// --- Property / fuzz tests --------------------------------------------------------------------
// The parser is a recover-and-continue recursive-descent parser: on *any* input it must report
// diagnostics rather than panic, and `parse()` must always succeed in producing a `ProgramNode`
// (it never returns `Err`). These tests throw large amounts of malformed input at it; reaching the
// end of each test (without a panic or hang) is the assertion.

/// Tiny deterministic xorshift PRNG so fuzz inputs are reproducible without external crates.
struct XorShift(u64);
impl XorShift {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn pick<'t>(&mut self, items: &[&'t str]) -> &'t str {
        items[(self.next_u64() as usize) % items.len()]
    }
}

/// Parses `code` and asserts the parser produced a `ProgramNode` without panicking or erroring.
fn assert_parses_without_panic(code: &str) {
    let arena = bumpalo::Bump::new();
    let mut diagnostics = DiagnosticBag::new(None);
    let lexer = Lexer::new(code.to_string());
    let mut parser = Parser::new(lexer, &arena, &mut diagnostics);
    let result = parser.parse();
    assert!(
        result.is_ok(),
        "parser returned Err (should always yield a ProgramNode) for input: {:?}",
        code
    );
}

#[test]
fn fuzz_random_token_soup_never_panics() {
    const TOKENS: [&str; 64] = [
        "fun",
        "class",
        "enum",
        "extend",
        "let",
        "const",
        "public",
        "static",
        "async",
        "return",
        "if",
        "else",
        "while",
        "for",
        "do",
        "switch",
        "case",
        "default",
        "break",
        "continue",
        "import",
        "type",
        "constructor",
        "del",
        "await",
        "true",
        "false",
        "null",
        "int",
        "string",
        "bool",
        "double",
        "float",
        "char",
        "void",
        "object",
        "{",
        "}",
        "(",
        ")",
        "[",
        "]",
        "<",
        ">",
        ":",
        ";",
        ",",
        ".",
        "=",
        "==",
        "+",
        "-",
        "*",
        "/",
        "%",
        "?",
        "@",
        "&&",
        "||",
        "\"s\"",
        "123",
        "3.14",
        "'c'",
        "ident",
    ];
    let mut rng = XorShift(0x9E3779B97F4A7C15);
    for _ in 0..3000 {
        let len = (rng.next_u64() as usize) % 40;
        let mut s = String::new();
        for _ in 0..len {
            s.push_str(rng.pick(&TOKENS));
            s.push(' ');
        }
        assert_parses_without_panic(&s);
    }
}

#[test]
fn fuzz_truncated_valid_programs_never_panic() {
    let samples = [
        "fun main(): int { return 42; }",
        "class Box<T> { public value: T; }",
        "public fun add(a: int, b: int): int { return a + b; }",
        "enum Color { Red, Green = 5, Blue }",
        "fun f() { let xs: int[] = [1,2,3]; for (x in xs) { System.println(x); } }",
        "extend int { public fun doubled(): int { return this * 2; } }",
        "const LIMIT: int = 5; let counter: int = LIMIT * 2;",
        "@json class User { public name: string; public age: int; }",
        "async fun g(): int { await sleep(1); return await h(); }",
    ];
    for s in samples {
        // Every byte prefix (a "file cut off mid-token") must still parse without panicking.
        for end in 0..=s.len() {
            if !s.is_char_boundary(end) {
                continue;
            }
            assert_parses_without_panic(&s[..end]);
        }
    }
}

#[test]
fn fuzz_byte_mutations_never_panic() {
    let base = "fun main(): int { let x = foo(1, 2); return x; }";
    let bytes = base.as_bytes();
    let mut rng = XorShift(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..3000 {
        let mut v = bytes.to_vec();
        let mutations = 1 + (rng.next_u64() as usize) % 6;
        for _ in 0..mutations {
            let idx = (rng.next_u64() as usize) % v.len();
            v[idx] = (rng.next_u64() as u8) | 0x20; // bias toward printable bytes
        }
        let s = String::from_utf8_lossy(&v);
        assert_parses_without_panic(&s);
    }
}

#[test]
fn fuzz_unbalanced_delimiters_never_panic() {
    let pieces = [
        "{", "}", "(", ")", "[", "]", "<", ">", "fun", "class", "if", "for", "x", ";", ":", ",",
    ];
    let mut rng = XorShift(0x1234_5678_ABCD_EF01);
    for _ in 0..4000 {
        let len = (rng.next_u64() as usize) % 60;
        let mut s = String::new();
        for _ in 0..len {
            s.push_str(rng.pick(&pieces));
        }
        assert_parses_without_panic(&s);
    }
}

#[test]
fn fuzz_recovers_and_reports_multiple_errors() {
    // Two independently-broken statements: a robust block parser should recover from the first
    // and still parse/lint the second (so we expect to keep the valid trailing statement).
    let code = "fun main(): void { let = ; let y = 5; @@@ ; return y; }";
    let arena = bumpalo::Bump::new();
    let (program, diagnostics) = parse_code(code, &arena);
    assert!(
        diagnostics.has_errors(),
        "malformed block should report diagnostics"
    );
    // The parser still produced a function (didn't discard the whole declaration).
    assert_eq!(
        program.functions.len(),
        1,
        "function should still be recovered"
    );
}
