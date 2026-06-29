//! A span-indexed symbol model built by walking the parsed document. The compiler's analyzer
//! keys symbol tables by scope and never records an offset->symbol mapping, so navigation
//! features (hover, go-to-definition, find-references, completion) are served from this
//! lightweight index instead. It is best-effort and tolerant of partially-broken trees.

use bumpalo::Bump;
use dream::driver::diagnostics::DiagnosticBag;
use dream::syntax::lexer::Lexer;
use dream::syntax::nodes::struct_node::StructDeclarationNode;
use dream::syntax::nodes::{ExpressionNode, FunctionNode, ProgramNode, StatementNode, Type};
use dream::syntax::parser::Parser;
use dream::syntax::token::syntax_token::SyntaxToken;
use std::collections::HashMap;

/// Sentinel scope id for declarations that live at file scope (functions, structs, enums).
const GLOBAL: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymKind {
    Function,
    Struct,
    Enum,
    EnumMember,
    Field,
    Method,
    Variable,
    Param,
    Type,
    Keyword,
}

#[derive(Debug, Clone)]
pub struct Decl {
    pub name: String,
    pub kind: SymKind,
    /// The signature or type detail (e.g. `fun foo()` or `let x: int`).
    pub detail: String,
    /// Markdown-ready doc comment extracted from trivia.
    pub doc_comment: Option<String>,
    pub start: usize,
    pub end: usize,
    /// Function scope id, or [`GLOBAL`] for file-scope declarations.
    pub scope: usize,
    /// Resolved type name for variables/params/fields, used to type member access.
    pub ty: Option<String>,
    pub is_main: bool,
}

#[derive(Debug, Clone)]
pub struct Ref {
    pub name: String,
    pub kind: SymKind,
    pub start: usize,
    pub end: usize,
    pub scope: usize,
    pub is_main: bool,
}

/// Distinguishes an inferred-type hint (rendered after a `let` name, e.g. `: int`) from a
/// parameter-name hint (rendered before a call argument, e.g. `x:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlayKind {
    Type,
    Parameter,
}

/// A single inlay hint: where to anchor it (byte offset), its label, and what kind it is (which
/// drives padding/placement in the LSP layer).
#[derive(Debug, Clone)]
pub struct InlayHintOut {
    pub offset: usize,
    pub label: String,
    pub kind: InlayKind,
}

/// The complete symbol model for one document. All positions are byte offsets into the source.
pub struct Index {
    pub decls: Vec<Decl>,
    pub refs: Vec<Ref>,
    pub inlay_hints: Vec<InlayHintOut>,
}

/// A located definition or reference (byte span + hover text).
pub struct Located {
    pub start: usize,
    pub end: usize,
    pub contents: String,
}

impl Index {
    /// Parses `text` and builds the symbol model. Tolerates parse errors by indexing whatever
    /// AST the parser manages to produce.
    pub fn build(file_path: Option<&str>, text: &str) -> Index {
        let arena = Bump::new();
        let mut scratch = DiagnosticBag::new(None);
        let lexer = Lexer::new(text.to_string());
        let mut parser = Parser::new(lexer, &arena, &mut scratch);

        let mut builder = Builder {
            decls: Vec::new(),
            refs: Vec::new(),
            inlay_hints: Vec::new(),
            next_scope: 0,
            is_main: true,
            fn_params: HashMap::new(),
            method_params: HashMap::new(),
            ctor_params: HashMap::new(),
            struct_fields: HashMap::new(),
        };
        if let Ok(ast) = parser.parse() {
            let program = ast.get_root();
            
            // Pass 1: Declare all file-level symbols for the main program
            builder.walk_program_for_imports(program);

            let mut acc = dream::driver::source_manager::ProgramAccumulator::default();

            // Inject standard library (prelude) symbols
            let mut file_contents = std::collections::HashMap::new();
            let _ = dream::driver::source_manager::merge_prelude(
                &arena,
                &mut acc.all_functions,
                &mut acc.all_structs,
                &mut acc.all_extends,
                &mut scratch,
                &mut file_contents,
            );

            if let Some(path_str) = file_path {
                let parent_dir = std::path::Path::new(path_str)
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(""));

                acc.visited.insert(path_str.to_string());

                for import in &program.imports {
                    let module_name = import.module_name.text.trim_matches('"');
                    let import_path =
                        dream::driver::source_manager::resolve_import_path(parent_dir, module_name);

                    if let Some(import_path_str) = import_path.to_str() {
                        if import_path.exists() {
                            let _ = dream::driver::source_manager::parse_file_recursive(
                                &import_path_str.to_string(),
                                &mut acc,
                                &arena,
                                &mut scratch,
                            );
                        }
                    }
                }
            }

            let combined = dream::syntax::nodes::ProgramNode::new(
                vec![],
                acc.all_structs,
                acc.all_functions,
                acc.all_enums,
                acc.all_extends,
            );
            // Pass 1.5: Declare all imported and prelude symbols
            builder.is_main = false;
            builder.walk_program_for_imports(&combined);
            builder.is_main = true;
            
            // Pass 2: Walk function/method bodies
            builder.walk_program(program);
        }
        Index {
            decls: builder.decls,
            refs: builder.refs,
            inlay_hints: builder.inlay_hints,
        }
    }

    fn span_at(start: usize, end: usize, offset: usize) -> bool {
        offset >= start && offset <= end
    }

    /// Returns the declaration whose name token is under `offset`, if any.
    fn decl_at(&self, offset: usize) -> Option<&Decl> {
        self.decls
            .iter()
            .find(|d| d.is_main && Self::span_at(d.start, d.end, offset))
    }

    /// Returns the reference whose name token is under `offset`, if any.
    fn ref_at(&self, offset: usize) -> Option<&Ref> {
        self.refs
            .iter()
            .find(|r| r.is_main && Self::span_at(r.start, r.end, offset))
    }

    /// Resolves a name used at `offset` within `scope` to its declaration. Locals (variables and
    /// parameters declared at or before the use site, in the same function) take precedence over
    /// file-scope declarations, approximating lexical scoping without block-level precision.
    fn resolve(&self, name: &str, scope: usize, before: usize) -> Option<&Decl> {
        let local = self
            .decls
            .iter()
            .filter(|d| {
                d.name == name
                    && d.scope == scope
                    && matches!(d.kind, SymKind::Variable | SymKind::Param)
                    && d.start <= before
            })
            .max_by_key(|d| d.start);
        if local.is_some() {
            return local;
        }
        self.decls.iter().find(|d| {
            d.name == name
                && d.scope == GLOBAL
                && matches!(d.kind, SymKind::Function | SymKind::Struct | SymKind::Enum)
        })
    }

    /// Resolves any field or method named `name` (the first match across all structs), used as a
    /// fallback for member access where the precise receiver type is unknown.
    fn resolve_member(&self, name: &str) -> Option<&Decl> {
        self.decls
            .iter()
            .find(|d| d.name == name && matches!(d.kind, SymKind::Field | SymKind::Method))
    }

    fn substitute_generic(detail: &str, receiver_ty: &str) -> String {
        // receiver_ty might be "List<int>" or "List_int" depending on how it was inferred.
        // Let's extract the generic part between `<` and `>` or after `_`.
        let mut generic_arg = None;
        if let Some(start) = receiver_ty.find('<') {
            if let Some(end) = receiver_ty.rfind('>') {
                generic_arg = Some(&receiver_ty[start + 1..end]);
            }
        } else if let Some(start) = receiver_ty.find('_') {
            generic_arg = Some(&receiver_ty[start + 1..]);
        }

        let Some(generic_arg) = generic_arg else {
            return detail.to_string();
        };

        // This is a naive substitution (replaces whole words).
        detail
            .replace("<T>", &format!("<{}>", generic_arg))
            .replace(": T", &format!(": {}", generic_arg))
            .replace(" T,", &format!(" {},", generic_arg))
            .replace(" T)", &format!(" {})", generic_arg))
            .replace(" T>", &format!(" {}>", generic_arg))
            .replace(" T ", &format!(" {} ", generic_arg))
    }

    pub fn hover(&self, offset: usize, text: &str) -> Option<Located> {
        let mut receiver_ty_opt = None;
        let (start, end, decl) = if let Some(decl) = self.decl_at(offset) {
            (decl.start, decl.end, decl)
        } else {
            let reference = self.ref_at(offset)?;
            let d = match reference.kind {
                SymKind::Field | SymKind::Method | SymKind::EnumMember => {
                    // Try to infer receiver type
                    let bytes = text.as_bytes();
                    let mut i = reference.start;
                    while i > 0 && is_ident_byte(bytes[i - 1]) {
                        i -= 1;
                    }
                    if i > 0 && bytes[i - 1] == b'.' {
                        let mut j = i - 1;
                        while j > 0 && bytes[j - 1] == b' ' { j -= 1; }
                        let recv_end = j;
                        let mut recv_start = recv_end;
                        while recv_start > 0 && is_ident_byte(bytes[recv_start - 1]) {
                            recv_start -= 1;
                        }
                        let receiver = &text[recv_start..recv_end];
                        receiver_ty_opt = self.variable_type(receiver, reference.scope, reference.start);
                    }
                    self.resolve_member(&reference.name)
                }
                _ => self.resolve(&reference.name, reference.scope, reference.start),
            }?;
            (reference.start, reference.end, d)
        };

        let mut detail = decl.detail.clone();
        if let Some(receiver_ty) = receiver_ty_opt {
            detail = Self::substitute_generic(&detail, &receiver_ty);
        }

        let mut contents = format!("```dream\n{}\n```", detail);
        if let Some(doc) = &decl.doc_comment {
            contents.push_str("\n\n---\n\n");
            contents.push_str(doc);
        }

        Some(Located {
            start,
            end,
            contents,
        })
    }

    pub fn definition(&self, offset: usize) -> Option<(usize, usize)> {
        if let Some(decl) = self.decl_at(offset) {
            return Some((decl.start, decl.end));
        }
        let reference = self.ref_at(offset)?;
        let decl = match reference.kind {
            SymKind::Field | SymKind::Method | SymKind::EnumMember => {
                self.resolve_member(&reference.name)
            }
            _ => self.resolve(&reference.name, reference.scope, reference.start),
        }?;
        Some((decl.start, decl.end))
    }

    pub fn signature_help(&self, text: &str, offset: usize) -> Option<Decl> {
        let bytes = text.as_bytes();
        let mut i = offset;
        let mut paren_count = 0;
        let mut open_paren_offset = None;

        while i > 0 {
            i -= 1;
            let b = bytes[i];
            if b == b')' {
                paren_count += 1;
            } else if b == b'(' {
                if paren_count > 0 {
                    paren_count -= 1;
                } else {
                    open_paren_offset = Some(i);
                    break;
                }
            } else if b == b';' || b == b'{' || b == b'}' {
                return None;
            }
        }

        let op_idx = open_paren_offset?;
        let mut j = op_idx;
        while j > 0 && (bytes[j - 1] == b' ' || bytes[j - 1] == b'\t' || bytes[j - 1] == b'\n') {
            j -= 1;
        }
        let recv_end = j;
        let mut recv_start = recv_end;
        while recv_start > 0 && is_ident_byte(bytes[recv_start - 1]) {
            recv_start -= 1;
        }

        if recv_start == recv_end {
            return None;
        }

        let name = &text[recv_start..recv_end];
        let scope = self.enclosing_scope(offset);

        let mut k = recv_start;
        while k > 0 && (bytes[k - 1] == b' ' || bytes[k - 1] == b'\t' || bytes[k - 1] == b'\n') {
            k -= 1;
        }
        if k > 0 && bytes[k - 1] == b'.' {
            let mut j2 = k - 1;
            while j2 > 0 && bytes[j2 - 1] == b' ' { j2 -= 1; }
            let recv_obj_end = j2;
            let mut recv_obj_start = recv_obj_end;
            while recv_obj_start > 0 && is_ident_byte(bytes[recv_obj_start - 1]) {
                recv_obj_start -= 1;
            }
            let receiver_obj = &text[recv_obj_start..recv_obj_end];
            let receiver_ty_opt = self.variable_type(receiver_obj, scope, recv_obj_start);

            if let Some(decl) = self.resolve_member(name) {
                let mut d = decl.clone();
                if let Some(receiver_ty) = receiver_ty_opt {
                    d.detail = Self::substitute_generic(&d.detail, &receiver_ty);
                }
                return Some(d);
            }
        } else {
            if let Some(decl) = self.resolve(name, scope, recv_start) {
                if decl.kind == SymKind::Struct {
                    if let Some(ctor_decl) = self.decls.iter().find(|d| {
                        d.name == "constructor"
                            && d.kind == SymKind::Method
                            && d.detail.starts_with(&format!("{}.", name))
                    }) {
                        return Some(ctor_decl.clone());
                    }
                } else {
                    return Some(decl.clone());
                }
            }
            // For struct initializers where `resolve` failed entirely (e.g. static imports sometimes)
            if let Some(decl) = self.decls.iter().find(|d| {
                d.name == "constructor"
                    && d.kind == SymKind::Method
                    && d.detail.starts_with(&format!("{}.", name))
            }) {
                return Some(decl.clone());
            }
        }

        None
    }

    /// Type name of a variable/parameter named `name` visible at `before` within `scope`.
    fn variable_type(&self, name: &str, scope: usize, before: usize) -> Option<String> {
        self.resolve(name, scope, before).and_then(|d| d.ty.clone())
    }

    /// Completion proposals at `offset`. After a `.` we attempt member completion against the
    /// receiver's resolved struct type, falling back to all members when the type is unknown.
    pub fn completions(
        &self,
        file_path: Option<&str>,
        text: &str,
        offset: usize,
    ) -> Vec<(String, SymKind, String, Option<String>)> {
        let scope = self.enclosing_scope(offset);
        let bytes = text.as_bytes();

        // Check for import path completion
        let mut i = offset;
        while i > 0 && bytes[i - 1] != b'"' && bytes[i - 1] != b'\n' {
            i -= 1;
        }
        if i > 0 && bytes[i - 1] == b'"' {
            let mut j = i - 1;
            while j > 0 && (bytes[j - 1] == b' ' || bytes[j - 1] == b'\t') {
                j -= 1;
            }
            if j >= 6 && &text[j - 6..j] == "import" {
                let mut out = Vec::new();
                if let Some(path_str) = file_path {
                    let parent_dir = std::path::Path::new(path_str)
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new(""));
                    let current_dir = if offset > i {
                        parent_dir.join(&text[i..offset])
                    } else {
                        parent_dir.to_path_buf()
                    };

                    let search_dir = if current_dir.is_dir() {
                        current_dir.clone()
                    } else {
                        current_dir
                            .parent()
                            .unwrap_or_else(|| std::path::Path::new(""))
                            .to_path_buf()
                    };

                    if let Ok(entries) = std::fs::read_dir(&search_dir) {
                        for entry in entries.flatten() {
                            if let Ok(file_type) = entry.file_type() {
                                let name = entry.file_name().to_string_lossy().to_string();
                                if file_type.is_dir() {
                                    out.push((
                                        name,
                                        SymKind::Variable,
                                        "directory".to_string(),
                                        None,
                                    ));
                                } else if name.ends_with(".dream") {
                                    out.push((name, SymKind::Variable, "module".to_string(), None));
                                }
                            }
                        }
                    }
                }
                return out;
            }
        }

        // Detect `receiver.<partial>` by scanning back over an identifier and a dot.
        let mut i = offset;
        while i > 0 && is_ident_byte(bytes[i - 1]) {
            i -= 1;
        }
        if i > 0 && bytes[i - 1] == b'.' {
            let mut j = i - 1;
            while j > 0 && bytes[j - 1] == b' ' {
                j -= 1;
            }
            let recv_end = j;
            let mut recv_start = recv_end;
            while recv_start > 0 && is_ident_byte(bytes[recv_start - 1]) {
                recv_start -= 1;
            }
            let receiver = &text[recv_start..recv_end];
            return self.member_completions(receiver, scope, recv_start);
        }

        let mut out = Vec::new();
        for kw in KEYWORDS {
            out.push((
                kw.to_string(),
                SymKind::Keyword,
                "keyword".to_string(),
                None,
            ));
        }
        for d in &self.decls {
            match d.kind {
                SymKind::Function | SymKind::Struct | SymKind::Enum => {
                    out.push((
                        d.name.clone(),
                        d.kind,
                        d.detail.clone(),
                        d.doc_comment.clone(),
                    ));
                }
                SymKind::Variable | SymKind::Param if d.scope == scope && d.start <= offset => {
                    out.push((
                        d.name.clone(),
                        d.kind,
                        d.detail.clone(),
                        d.doc_comment.clone(),
                    ));
                }
                _ => {}
            }
        }
        out
    }

    /// Members available on `receiver`, resolved by type. If `receiver` is a variable/parameter
    /// (including `this`) whose type is a known struct, only that struct's fields and methods are
    /// offered. If `receiver` names an enum, its members are offered. Otherwise nothing is
    /// offered, so member access never dumps unrelated symbols.
    fn member_completions(
        &self,
        receiver: &str,
        scope: usize,
        before: usize,
    ) -> Vec<(String, SymKind, String, Option<String>)> {
        // `Type.` / `Color.` static or enum access: the receiver itself names a struct or enum.
        if self
            .decls
            .iter()
            .any(|d| d.kind == SymKind::Enum && d.name == receiver)
        {
            return self.members_of_enum(receiver);
        }

        if let Some(ty) = self.variable_type(receiver, scope, before) {
            let base = ty.trim_end_matches('?').trim_end_matches("[]");
            return self.members_of_struct(base);
        }

        // A bare struct name used as a receiver (e.g. static method access `Point.`).
        if self
            .decls
            .iter()
            .any(|d| d.kind == SymKind::Struct && d.name == receiver)
        {
            return self.members_of_struct(receiver);
        }

        Vec::new()
    }

    fn members_of_struct(&self, base: &str) -> Vec<(String, SymKind, String, Option<String>)> {
        let prefix = format!("{}.", base);
        self.decls
            .iter()
            .filter(|d| {
                matches!(d.kind, SymKind::Field | SymKind::Method)
                    && d.scope == GLOBAL
                    && d.detail.starts_with(&prefix)
                    && d.name != "constructor"
            })
            .map(|d| {
                let detail = Self::substitute_generic(&d.detail, base);
                (
                    d.name.clone(),
                    d.kind,
                    detail,
                    d.doc_comment.clone(),
                )
            })
            .collect()
    }

    fn members_of_enum(&self, name: &str) -> Vec<(String, SymKind, String, Option<String>)> {
        let prefix = format!("{}.", name);
        self.decls
            .iter()
            .filter(|d| d.kind == SymKind::EnumMember && d.detail.starts_with(&prefix))
            .map(|d| {
                (
                    d.name.clone(),
                    d.kind,
                    d.detail.clone(),
                    d.doc_comment.clone(),
                )
            })
            .collect()
    }

    /// The function scope whose body span contains `offset`, or [`GLOBAL`].
    fn enclosing_scope(&self, offset: usize) -> usize {
        // Parameters/locals of a function share its scope id and are appended in source order,
        // so the latest local/param declared before `offset` identifies the enclosing function.
        let mut best: Option<(usize, usize)> = None; // (scope, name_start)
        for d in &self.decls {
            if matches!(d.kind, SymKind::Param | SymKind::Variable)
                && d.scope != GLOBAL
                && d.start <= offset
            {
                match best {
                    Some((_, s)) if s >= d.start => {}
                    _ => best = Some((d.scope, d.start)),
                }
            }
        }
        best.map(|(scope, _)| scope).unwrap_or(GLOBAL)
    }
}

struct Builder {
    decls: Vec<Decl>,
    refs: Vec<Ref>,
    inlay_hints: Vec<InlayHintOut>,
    next_scope: usize,
    is_main: bool,
    /// Parameter names per free function name, used to render parameter-name inlay hints at calls.
    fn_params: HashMap<String, Vec<String>>,
    /// Parameter names per method name (the implicit `this` is not a parsed parameter).
    method_params: HashMap<String, Vec<String>>,
    /// Constructor parameter names per struct name (only when a custom `constructor` is declared).
    ctor_params: HashMap<String, Vec<String>>,
    /// Field names per struct name, in declaration order. These are the positional arguments of a
    /// struct's auto-generated constructor (when it has no custom `constructor`).
    struct_fields: HashMap<String, Vec<String>>,
}

impl Builder {
    fn infer_type(&self, expr: &ExpressionNode, scope: usize) -> Option<String> {
        let ty = self.infer_type_internal(expr, scope);
        ty
    }

    fn infer_type_internal(&self, expr: &ExpressionNode, scope: usize) -> Option<String> {
        match expr {
            ExpressionNode::Literal(t) => Some(t.get_type()),
            ExpressionNode::Cast(ty, _) => Some(ty.get_type()),
            ExpressionNode::IsExpression(_, _) => Some("bool".to_string()),
            ExpressionNode::Binary(_, op, _) => {
                match op.kind {
                    dream::syntax::token::token_kind::TokenKind::EqualEqualToken
                    | dream::syntax::token::token_kind::TokenKind::NotEqualToken
                    | dream::syntax::token::token_kind::TokenKind::GreaterThanToken
                    | dream::syntax::token::token_kind::TokenKind::GreaterThanEqualToken
                    | dream::syntax::token::token_kind::TokenKind::SmallerThanToken
                    | dream::syntax::token::token_kind::TokenKind::SmallerThanEqualToken
                    | dream::syntax::token::token_kind::TokenKind::AmpersandAmpersandToken
                    | dream::syntax::token::token_kind::TokenKind::PipePipeToken => Some("bool".to_string()),
                    _ => None,
                }
            }
            ExpressionNode::Identifier(token) => {
                self.resolve(&token.text, scope, token.position.start).and_then(|d| d.ty.clone())
            }
            ExpressionNode::MemberAccess(_recv, member) => {
                // To properly type `obj.field`, we'd resolve `obj`'s type, then find the field in that struct.
                // For a simple heuristic, just find *any* field with this name.
                self.decls.iter()
                    .find(|d| d.name == member.text && d.kind == SymKind::Field)
                    .and_then(|d| d.ty.clone())
            }
            ExpressionNode::FunctionCall(name, generic_args, _) => {
                self.resolve(&name.text, scope, name.position.start).and_then(|d| {
                    if d.kind == SymKind::Struct {
                        // It's a constructor call (e.g. `Test("John", 20)`), so the type is the struct name itself.
                        match generic_args {
                            Some(args) => Some(dream::syntax::nodes::types::mangle_generic(&name.text, args)),
                            None => Some(name.text.clone()),
                        }
                    } else {
                        // detail string usually looks like: fun(int, int): string
                        if let Some(colon_idx) = d.detail.rfind(':') {
                            let mut ret_ty = d.detail[colon_idx + 1..].trim().to_string();
                            if let Some(args) = generic_args {
                                if args.len() == 1 {
                                    let arg_type = args[0].get_type();
                                    ret_ty = ret_ty.replace("_T", &format!("_{}", arg_type))
                                                   .replace("<T>", &format!("<{}>", arg_type))
                                                   .replace(" T", &format!(" {}", arg_type))
                                                   .replace("T ", &format!("{} ", arg_type));
                                    if ret_ty == "T" {
                                        ret_ty = arg_type.to_string();
                                    }
                                }
                            }
                            Some(ret_ty)
                        } else {
                            None
                        }
                    }
                })
            }
            ExpressionNode::MethodCall(recv, method, _, _) => {
                let receiver_ty_opt = self.infer_type(recv, scope);
                self.decls.iter()
                    .find(|d| d.name == method.text && d.kind == SymKind::Method)
                    .and_then(|d| {
                        let detail = if let Some(receiver_ty) = &receiver_ty_opt {
                            Index::substitute_generic(&d.detail, receiver_ty)
                        } else {
                            d.detail.clone()
                        };
                        if let Some(colon_idx) = detail.rfind(':') {
                            Some(detail[colon_idx + 1..].trim().to_string())
                        } else {
                            None
                        }
                    })
            }
            ExpressionNode::Parenthesized(inner) => self.infer_type(inner, scope),
            _ => None,
        }
    }

    fn resolve(&self, name: &str, scope: usize, before: usize) -> Option<&Decl> {
        let local = self
            .decls
            .iter()
            .filter(|d| {
                d.name == name
                    && d.scope == scope
                    && matches!(d.kind, SymKind::Variable | SymKind::Param)
                    && d.start <= before
            })
            .max_by_key(|d| d.start);
        if local.is_some() {
            return local;
        }
        self.decls.iter().find(|d| {
            d.name == name
                && d.scope == GLOBAL
                && matches!(d.kind, SymKind::Function | SymKind::Struct | SymKind::Enum)
        })
    }

    fn walk_program_for_imports(&mut self, program: &ProgramNode) {
        for func in &program.functions {
            let detail = signature(func);
            self.push_decl(&func.name, SymKind::Function, detail, GLOBAL, None);
            self.fn_params
                .insert(func.name.text.clone(), param_names(func));
        }
        for st in &program.structs {
            let detail = format!("class {}", st.name.text);
            self.push_decl(&st.name, SymKind::Struct, detail, GLOBAL, None);
            self.struct_fields.insert(
                st.name.text.clone(),
                st.fields.iter().map(|f| f.name.text.clone()).collect(),
            );
            for field in &st.fields {
                let detail = format!(
                    "{}.{}: {}",
                    st.name.text, field.name.text, field.type_token.text
                );
                self.push_decl(
                    &field.name,
                    SymKind::Field,
                    detail,
                    GLOBAL,
                    Some(field.type_token.text.clone()),
                );
            }
            for method in &st.methods {
                let detail = format!("{}.{}", st.name.text, signature(method));
                self.push_decl(&method.name, SymKind::Method, detail, GLOBAL, None);
                if method.name.text == "constructor" {
                    self.ctor_params
                        .insert(st.name.text.clone(), param_names(method));
                } else {
                    self.method_params
                        .insert(method.name.text.clone(), param_names(method));
                }
            }
        }
        for en in &program.enums {
            let detail = format!("enum {}", en.name.text);
            self.push_decl(&en.name, SymKind::Enum, detail, GLOBAL, None);
            for (member, value) in &en.members {
                let detail = format!("{}.{} = {}", en.name.text, member.text, value);
                self.push_decl(member, SymKind::EnumMember, detail, GLOBAL, None);
            }
        }
        for ext in &program.extends {
            for method in &ext.methods {
                let detail = format!("{}.{}", ext.target.text, signature(method));
                self.push_decl(&method.name, SymKind::Method, detail, GLOBAL, None);
                self.method_params
                    .insert(method.name.text.clone(), param_names(method));
            }
        }
    }

    fn walk_attributes(&mut self, attributes: &[dream::syntax::nodes::AttributeNode], scope: usize) {
        for attr in attributes {
            // Treat the attribute name as a reference to a struct/class (even if it's currently a built-in).
            self.add_ref(&attr.name, SymKind::Struct, scope);
            for arg in &attr.args {
                // If the argument is an identifier (e.g. referencing a constant), record it.
                if arg.kind == dream::syntax::token::token_kind::TokenKind::IdentifierToken {
                    self.add_ref(arg, SymKind::Variable, scope);
                }
            }
        }
    }

    fn walk_program(&mut self, program: &ProgramNode) {
        for func in &program.functions {
            self.walk_attributes(&func.attributes, GLOBAL);
            self.walk_function(func, None);
        }
        for st in &program.structs {
            self.walk_attributes(&st.attributes, GLOBAL);
            for field in &st.fields {
                self.walk_attributes(&field.attributes, GLOBAL);
            }
            self.walk_struct(st);
        }
        for _en in &program.enums {
            // Already declared in pass 1
        }
        for ext in &program.extends {
            for method in &ext.methods {
                self.walk_attributes(&method.attributes, GLOBAL);
                self.walk_method(method, &ext.target.text);
            }
        }
    }

    fn walk_struct(&mut self, st: &StructDeclarationNode) {
        for method in &st.methods {
            self.walk_method(method, &st.name.text);
        }
    }

    fn walk_method(&mut self, func: &FunctionNode, owner: &str) {
        let scope = self.fresh_scope();
        // Instance methods receive an implicit `this` bound to the owning type, so member
        // access on `this` can be resolved to the owner's fields/methods. Static methods do not.
        if !func.is_static {
            self.decls.push(Decl {
                name: "this".to_string(),
                kind: SymKind::Param,
                detail: format!("(this) {}", owner),
                doc_comment: None,
                start: func.name.position.start,
                end: func.name.position.end,
                scope,
                ty: Some(owner.to_string()),
                is_main: self.is_main,
            });
        }
        self.walk_params_and_body(func, scope);
    }

    fn walk_function(&mut self, func: &FunctionNode, _owner: Option<&str>) {
        let scope = self.fresh_scope();
        self.walk_params_and_body(func, scope);
    }

    fn walk_params_and_body(&mut self, func: &FunctionNode, scope: usize) {
        for param in &func.parameters {
            let ty = param.type_.get_type();
            let detail = format!("(parameter) {}: {}", param.name.text, ty);
            self.push_decl(&param.name, SymKind::Param, detail, scope, Some(ty));
            self.add_type_ref(&param.type_, scope);
        }
        if let Some(rt) = &func.return_type {
            self.add_type_ref(rt, scope);
        }
        for stmt in func.body {
            self.walk_stmt(stmt, scope);
        }
    }

    fn walk_stmt(&mut self, stmt: &StatementNode, scope: usize) {
        match stmt {
            StatementNode::Declaration(name, ty, expr, _is_const) => {
                let inferred = self.infer_type(expr, scope);
                let type_str = ty
                    .as_ref()
                    .map(|t| t.get_type())
                    .or_else(|| inferred.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                let detail = type_str.clone();
                let resolved_ty = ty.as_ref().map(|t| t.get_type()).or(inferred);
                self.push_decl(name, SymKind::Variable, detail, scope, resolved_ty.clone());
                if let Some(t) = ty {
                    self.add_type_ref(t, scope);
                } else if let Some(t_str) = resolved_ty {
                    self.inlay_hints.push(InlayHintOut {
                        offset: name.position.end,
                        label: format!(": {}", t_str),
                        kind: InlayKind::Type,
                    });
                }
                self.walk_expr(expr, scope);
            }
            StatementNode::Assignment(name, expr) => {
                self.add_ref(name, SymKind::Variable, scope);
                self.walk_expr(expr, scope);
            }
            StatementNode::IndexAssignment(target, index, value) => {
                self.walk_expr(target, scope);
                self.walk_expr(index, scope);
                self.walk_expr(value, scope);
            }
            StatementNode::MemberAssignment(target, member, value) => {
                self.walk_expr(target, scope);
                self.add_ref(member, SymKind::Field, scope);
                self.walk_expr(value, scope);
            }
            StatementNode::Return(Some(expr)) => self.walk_expr(expr, scope),
            StatementNode::Return(None) => {}
            StatementNode::FunctionInvocation(name, _, args) => {
                self.add_ref(name, SymKind::Function, scope);
                let params = self.fn_params.get(&name.text).or_else(|| {
                    self.ctor_params
                        .get(&name.text)
                        .or_else(|| self.struct_fields.get(&name.text))
                });
                if let Some(params) = params {
                    self.push_param_hints(&params.clone(), args);
                }
                for arg in args {
                    self.walk_expr(arg, scope);
                }
            }
            StatementNode::ExpressionStatement(expr) => {
                self.walk_expr(expr, scope);
            }
            StatementNode::MethodInvocation(recv, method, _, args) => {
                self.walk_expr(recv, scope);
                self.add_ref(method, SymKind::Method, scope);
                if let Some(params) = self.method_params.get(&method.text) {
                    self.push_param_hints(&params.clone(), args);
                }
                for arg in args {
                    self.walk_expr(arg, scope);
                }
            }
            StatementNode::IfElse(cond, then_body, else_ifs, else_body) => {
                self.walk_expr(cond, scope);
                self.walk_block(then_body, scope);
                for (c, body) in else_ifs {
                    self.walk_expr(c, scope);
                    self.walk_block(body, scope);
                }
                if let Some(body) = else_body {
                    self.walk_block(body, scope);
                }
            }
            StatementNode::While(cond, body) => {
                self.walk_expr(cond, scope);
                self.walk_block(body, scope);
            }
            StatementNode::DoWhile(body, cond) => {
                self.walk_block(body, scope);
                self.walk_expr(cond, scope);
            }
            StatementNode::For(init, cond, update, body) => {
                if let Some(s) = init {
                    self.walk_stmt(s, scope);
                }
                if let Some(c) = cond {
                    self.walk_expr(c, scope);
                }
                if let Some(s) = update {
                    self.walk_stmt(s, scope);
                }
                self.walk_block(body, scope);
            }
            StatementNode::ForEach(var, iterable, _, _, body) => {
                let detail = "unknown".to_string();
                self.push_decl(var, SymKind::Variable, detail, scope, None);
                self.walk_expr(iterable, scope);
                self.walk_block(body, scope);
            }
            StatementNode::Labeled(_, inner) => self.walk_stmt(inner, scope),
            StatementNode::AwaitStmt(expr) => self.walk_expr(expr, scope),
            StatementNode::Break(_) | StatementNode::Continue(_) => {}
            StatementNode::Switch(subject, cases, default) => {
                self.walk_expr(subject, scope);
                for (labels, body) in cases {
                    for label in labels {
                        self.walk_expr(label, scope);
                    }
                    self.walk_block(body, scope);
                }
                if let Some(body) = default {
                    self.walk_block(body, scope);
                }
            }
        }
    }

    fn walk_block(&mut self, body: &[StatementNode], scope: usize) {
        for stmt in body {
            self.walk_stmt(stmt, scope);
        }
    }

    /// Emits a parameter-name inlay hint (`name:`) before each positional argument of a call. The
    /// hint is suppressed when the argument is simply the identifier matching the parameter name,
    /// which would be redundant. Extra arguments (more than parameters) are left unannotated.
    fn push_param_hints(&mut self, params: &[String], args: &[ExpressionNode]) {
        for (param, arg) in params.iter().zip(args.iter()) {
            if let ExpressionNode::Identifier(tok) = arg {
                if &tok.text == param {
                    continue;
                }
            }
            if let Some(span) = arg.position() {
                self.inlay_hints.push(InlayHintOut {
                    offset: span.start,
                    label: format!("{}:", param),
                    kind: InlayKind::Parameter,
                });
            }
        }
    }

    fn walk_expr(&mut self, expr: &ExpressionNode, scope: usize) {
        match expr {
            ExpressionNode::Identifier(token) => self.add_ref(token, SymKind::Variable, scope),
            ExpressionNode::Binary(l, _, r) => {
                self.walk_expr(l, scope);
                self.walk_expr(r, scope);
            }
            ExpressionNode::Unary(_, e) | ExpressionNode::Parenthesized(e) => {
                self.walk_expr(e, scope)
            }
            ExpressionNode::FunctionCall(name, _, args) => {
                self.add_ref(name, SymKind::Function, scope);
                // A name resolves to a free function if one exists; otherwise `Name(...)` is a
                // constructor call, whose positional arguments are the custom `constructor`'s
                // parameters (if any) or the struct's fields in declaration order.
                let params = self.fn_params.get(&name.text).or_else(|| {
                    self.ctor_params
                        .get(&name.text)
                        .or_else(|| self.struct_fields.get(&name.text))
                });
                if let Some(params) = params {
                    self.push_param_hints(&params.clone(), args);
                }
                for arg in args {
                    self.walk_expr(arg, scope);
                }
            }
            ExpressionNode::IndexAccess(arr, idx) => {
                self.walk_expr(arr, scope);
                self.walk_expr(idx, scope);
            }
            ExpressionNode::Cast(ty, e) => {
                self.add_type_ref(ty, scope);
                self.walk_expr(e, scope);
            }
            ExpressionNode::MemberAccess(recv, member) => {
                self.walk_expr(recv, scope);
                // `Enum.Member` looks like member access on an identifier naming the enum.
                let kind = match recv {
                    ExpressionNode::Identifier(id) if self.is_enum(&id.text) => SymKind::EnumMember,
                    _ => SymKind::Field,
                };
                self.add_ref(member, kind, scope);
            }
            ExpressionNode::MethodCall(recv, method, _, args) => {
                self.walk_expr(recv, scope);
                self.add_ref(method, SymKind::Method, scope);
                if let Some(params) = self.method_params.get(&method.text) {
                    self.push_param_hints(&params.clone(), args);
                }
                for arg in args {
                    self.walk_expr(arg, scope);
                }
            }
            ExpressionNode::IsExpression(e, ty) => {
                self.walk_expr(e, scope);
                self.add_type_ref(ty, scope);
            }
            ExpressionNode::Ternary(c, t, e) => {
                self.walk_expr(c, scope);
                self.walk_expr(t, scope);
                self.walk_expr(e, scope);
            }
            ExpressionNode::ArrayLiteral(elems) => {
                for elem in elems {
                    self.walk_expr(elem, scope);
                }
            }
            ExpressionNode::Await(e) => self.walk_expr(e, scope),
            ExpressionNode::Literal(_) => {}
        }
    }

    fn add_type_ref(&mut self, ty: &Type, scope: usize) {
        if let Type::Struct(token, _) = base_struct(ty) {
            self.add_ref(token, SymKind::Type, scope);
        }
    }

    fn is_enum(&self, name: &str) -> bool {
        self.decls
            .iter()
            .any(|d| d.kind == SymKind::Enum && d.name == name)
    }

    fn fresh_scope(&mut self) -> usize {
        let scope = self.next_scope;
        self.next_scope += 1;
        scope
    }

    fn push_decl(
        &mut self,
        token: &SyntaxToken,
        kind: SymKind,
        detail: String,
        scope: usize,
        ty: Option<String>,
    ) {
        if token.text.is_empty() {
            return;
        }

        let mut doc_comment = None;

        // Append any leading doc comments to the hover detail
        for trivia in &token.leading_trivia {
            if trivia.kind == dream::syntax::token::token_kind::TokenKind::LineCommentToken
                || trivia.kind == dream::syntax::token::token_kind::TokenKind::BlockCommentToken
            {
                let mut text = trivia.text.trim();
                if text.starts_with("//") {
                    text = text.trim_start_matches('/').trim_start();
                } else if text.starts_with("/*") {
                    text = text.trim_start_matches("/*").trim_end_matches("*/").trim();
                }

                let comment = doc_comment.get_or_insert_with(String::new);
                if !comment.is_empty() {
                    comment.push_str("\n\n");
                }
                comment.push_str(text);
            }
        }
        self.decls.push(Decl {
            name: token.text.clone(),
            kind,
            detail,
            doc_comment,
            start: token.position.start,
            end: token.position.end,
            scope,
            ty,
            is_main: self.is_main,
        });
    }

    fn add_ref(&mut self, token: &SyntaxToken, kind: SymKind, scope: usize) {
        if token.text.is_empty() {
            return;
        }
        self.refs.push(Ref {
            name: token.text.clone(),
            kind,
            start: token.position.start,
            end: token.position.end,
            scope,
            is_main: self.is_main,
        });
    }
}

/// Returns the innermost struct type backing `ty` (peeling arrays and nullables), if any.
fn base_struct(ty: &Type) -> &Type {
    match ty {
        Type::Array(inner) | Type::Nullable(inner) => base_struct(inner),
        other => other,
    }
}

/// The parameter names of a function/method in declaration order (the implicit method `this` is
/// not a parsed parameter, so it never appears here).
fn param_names(func: &FunctionNode) -> Vec<String> {
    func.parameters
        .iter()
        .map(|p| p.name.text.clone())
        .collect()
}

/// Renders a function declaration's signature, e.g. `fun add(a: int, b: int): int`.
fn signature(func: &FunctionNode) -> String {
    let params = func
        .parameters
        .iter()
        .map(|p| format!("{}: {}", p.name.text, p.type_.get_type()))
        .collect::<Vec<_>>()
        .join(", ");
    let ret = func
        .return_type
        .as_ref()
        .map(|t| t.get_type())
        .unwrap_or_else(|| "void".to_string());

    let prefix = if func.is_async { "async fun " } else { "fun " };

    if func.name.text == "constructor" || func.name.text == "del" {
        format!("{}({}): {}", func.name.text, params, ret)
    } else {
        format!("{}{}({}): {}", prefix, func.name.text, params, ret)
    }
}

fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Language keywords offered as completion proposals.
pub const KEYWORDS: [&str; 37] = [
    "if",
    "else",
    "for",
    "while",
    "do",
    "return",
    "break",
    "continue",
    "let",
    "const",
    "fun",
    "static",
    "import",
    "export",
    "extern",
    "class",
    "extend",
    "enum",
    "type",
    "switch",
    "case",
    "default",
    "is",
    "in",
    "true",
    "false",
    "null",
    "constructor",
    "del",
    "int",
    "float",
    "double",
    "string",
    "bool",
    "char",
    "void",
    "object",
];
