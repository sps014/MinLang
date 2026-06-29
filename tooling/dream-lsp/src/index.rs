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
}

#[derive(Debug, Clone)]
pub struct Ref {
    pub name: String,
    pub kind: SymKind,
    pub start: usize,
    pub end: usize,
    pub scope: usize,
}

/// The complete symbol model for one document. All positions are byte offsets into the source.
pub struct Index {
    pub decls: Vec<Decl>,
    pub refs: Vec<Ref>,
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
            next_scope: 0,
        };
        if let Ok(ast) = parser.parse() {
            let program = ast.get_root();
            builder.walk_program(program);

            if let Some(path_str) = file_path {
                let parent_dir = std::path::Path::new(path_str)
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(""));
                let mut visited = std::collections::HashSet::new();
                visited.insert(path_str.to_string());

                let mut all_functions = Vec::new();
                let mut all_structs = Vec::new();
                let mut all_enums = Vec::new();
                let mut all_extends = Vec::new();
                let mut file_contents = std::collections::HashMap::new();

                for import in &program.imports {
                    let module_name = import.module_name.text.trim_matches('"');
                    let mut import_path = parent_dir.join(module_name);
                    if import_path.extension().is_none() {
                        import_path.set_extension("dream");
                    }
                    if let Some(import_path_str) = import_path.to_str() {
                        if import_path.exists() {
                            let _ = dream::driver::source_manager::parse_file_recursive(
                                &import_path_str.to_string(),
                                &mut visited,
                                &mut all_functions,
                                &mut all_structs,
                                &mut all_enums,
                                &mut all_extends,
                                &arena,
                                &mut scratch,
                                &mut file_contents,
                            );
                        }
                    }
                }

                let combined =
                    ProgramNode::new(vec![], all_structs, all_functions, all_enums, all_extends);
                builder.walk_program_for_imports(&combined);
            }
        }
        Index {
            decls: builder.decls,
            refs: builder.refs,
        }
    }

    fn span_at(start: usize, end: usize, offset: usize) -> bool {
        offset >= start && offset <= end
    }

    /// Returns the declaration whose name token is under `offset`, if any.
    fn decl_at(&self, offset: usize) -> Option<&Decl> {
        self.decls
            .iter()
            .find(|d| Self::span_at(d.start, d.end, offset))
    }

    /// Returns the reference whose name token is under `offset`, if any.
    fn ref_at(&self, offset: usize) -> Option<&Ref> {
        self.refs
            .iter()
            .find(|r| Self::span_at(r.start, r.end, offset))
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

    pub fn hover(&self, offset: usize) -> Option<Located> {
        let (start, end, decl) = if let Some(decl) = self.decl_at(offset) {
            (decl.start, decl.end, decl)
        } else {
            let reference = self.ref_at(offset)?;
            let decl = match reference.kind {
                SymKind::Field | SymKind::Method | SymKind::EnumMember => {
                    self.resolve_member(&reference.name)
                }
                _ => self.resolve(&reference.name, reference.scope, reference.start),
            }?;
            (reference.start, reference.end, decl)
        };

        let mut contents = format!("```dream\n{}\n```", decl.detail);
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
            if let Some(decl) = self.resolve_member(name) {
                return Some(decl.clone());
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
                    let parent_dir = std::path::Path::new(path_str).parent().unwrap_or_else(|| std::path::Path::new(""));
                    let current_dir = if offset > i {
                        parent_dir.join(&text[i..offset])
                    } else {
                        parent_dir.to_path_buf()
                    };
                    
                    let search_dir = if current_dir.is_dir() {
                        current_dir.clone()
                    } else {
                        current_dir.parent().unwrap_or_else(|| std::path::Path::new("")).to_path_buf()
                    };

                    if let Ok(entries) = std::fs::read_dir(&search_dir) {
                        for entry in entries.flatten() {
                            if let Ok(file_type) = entry.file_type() {
                                let name = entry.file_name().to_string_lossy().to_string();
                                if file_type.is_dir() {
                                    out.push((name, SymKind::Variable, "directory".to_string(), None));
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
            })
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
    next_scope: usize,
}

impl Builder {
    fn walk_program_for_imports(&mut self, program: &ProgramNode) {
        for func in &program.functions {
            let detail = signature(func);
            self.push_decl(&func.name, SymKind::Function, detail, GLOBAL, None);
        }
        for st in &program.structs {
            let detail = format!("class {}", st.name.text);
            self.push_decl(&st.name, SymKind::Struct, detail, GLOBAL, None);
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
            }
        }
    }

    fn walk_program(&mut self, program: &ProgramNode) {
        for func in &program.functions {
            self.walk_function(func, None);
        }
        for st in &program.structs {
            self.walk_struct(st);
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
                self.walk_method(method, &ext.target.text);
            }
        }
    }

    fn walk_struct(&mut self, st: &StructDeclarationNode) {
        let detail = format!("class {}", st.name.text);
        self.push_decl(&st.name, SymKind::Struct, detail, GLOBAL, None);
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
            self.walk_method(method, &st.name.text);
        }
    }

    fn walk_method(&mut self, func: &FunctionNode, owner: &str) {
        let scope = self.fresh_scope();
        let detail = format!("{}.{}", owner, signature(func));
        self.push_decl(&func.name, SymKind::Method, detail, GLOBAL, None);
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
            });
        }
        self.walk_params_and_body(func, scope);
    }

    fn walk_function(&mut self, func: &FunctionNode, _owner: Option<&str>) {
        let scope = self.fresh_scope();
        let detail = signature(func);
        self.push_decl(&func.name, SymKind::Function, detail, GLOBAL, None);
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
            StatementNode::Declaration(name, ty, expr, is_const) => {
                let type_str = ty
                    .as_ref()
                    .map(|t| t.get_type())
                    .unwrap_or_else(|| "var".to_string());
                let keyword = if *is_const { "const" } else { "let" };
                let detail = format!("{} {}: {}", keyword, name.text, type_str);
                let resolved_ty = ty.as_ref().map(|t| t.get_type());
                self.push_decl(name, SymKind::Variable, detail, scope, resolved_ty);
                if let Some(t) = ty {
                    self.add_type_ref(t, scope);
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
                for arg in args {
                    self.walk_expr(arg, scope);
                }
            }
            StatementNode::MethodInvocation(recv, method, _, args) => {
                self.walk_expr(recv, scope);
                self.add_ref(method, SymKind::Method, scope);
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
                let detail = format!("let {}", var.text);
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
            ExpressionNode::StructInstantiation(name, _, fields) => {
                self.add_ref(name, SymKind::Struct, scope);
                for (field, value) in fields {
                    self.add_ref(field, SymKind::Field, scope);
                    self.walk_expr(value, scope);
                }
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
                    text = text.trim_start_matches("//").trim_start();
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

    if func.name.text == "constructor" || func.name.text == "del" {
        format!("{}({}): {}", func.name.text, params, ret)
    } else {
        format!("fun {}({}): {}", func.name.text, params, ret)
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
