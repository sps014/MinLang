use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::path::Path;
use bumpalo::Bump;

use crate::syntax::lexer::Lexer;
use crate::syntax::parser::Parser;
use crate::driver::diagnostics::DiagnosticBag;

/// Parses the embedded standard-collections prelude and merges its declarations into the
/// program. Uses the same arena as the user's files so all AST nodes share a lifetime.
pub(crate) fn merge_prelude<'a>(
    arena: &'a Bump,
    all_functions: &mut Vec<crate::syntax::nodes::FunctionNode<'a>>,
    all_structs: &mut Vec<crate::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
    diagnostics: &mut DiagnosticBag,
    file_contents: &mut HashMap<String, String>,
) -> Result<(), Error> {
    // Each standard-collection type lives in its own prelude file.
    const PRELUDE_FILES: [(&str, &str); 2] = [
        ("<std>/list.dream", include_str!("../stdlib/list.dream")),
        ("<std>/map.dream", include_str!("../stdlib/map.dream")),
    ];

    for (prelude_name, prelude_src) in PRELUDE_FILES {
        let prelude_name = prelude_name.to_string();
        file_contents.insert(prelude_name.clone(), prelude_src.to_string());

        let mut prelude_diagnostics = DiagnosticBag::new(Some(prelude_name.clone()));
        let lexer = Lexer::new(prelude_src.to_string());
        let mut parser = Parser::new(lexer, arena, &mut prelude_diagnostics);
        let ast = match parser.parse() {
            Ok(ast) => ast,
            Err(e) => {
                diagnostics.extend(&prelude_diagnostics);
                return Err(e);
            }
        };
        diagnostics.extend(&prelude_diagnostics);

        let program = ast.get_root();
        let file_tag: std::rc::Rc<str> = std::rc::Rc::from(prelude_name.as_str());
        for function in program.functions.iter().cloned() {
            let mut function = function;
            function.file_path = Some(file_tag.clone());
            all_functions.push(function);
        }
        for struct_decl in program.structs.iter().cloned() {
            let mut struct_decl = struct_decl;
            struct_decl.file_path = Some(file_tag.clone());
            for method in struct_decl.methods.iter_mut() {
                method.file_path = Some(file_tag.clone());
            }
            all_structs.push(struct_decl);
        }
    }

    Ok(())
}

/// Recursively parses `file_path` and every file it imports, merging all declarations into the
/// `all_*` accumulators. Each declaration is tagged with its originating file so semantic
/// diagnostics (which run on the merged program) can attribute errors correctly.
pub(crate) fn parse_file_recursive<'a>(
    file_path: &String,
    visited: &mut HashSet<String>,
    all_functions: &mut Vec<crate::syntax::nodes::FunctionNode<'a>>,
    all_structs: &mut Vec<crate::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
    all_enums: &mut Vec<crate::syntax::nodes::EnumDeclarationNode>,
    arena: &'a Bump,
    diagnostics: &mut DiagnosticBag,
    file_contents: &mut HashMap<String, String>,
) -> Result<(), Error> {
    let path = Path::new(file_path).canonicalize()?;
    let path_str = path.to_str()
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, format!("Non-UTF-8 file path: {:?}", path)))?
        .to_string();

    if visited.contains(&path_str) {
        return Ok(()); // Already processed
    }
    visited.insert(path_str.clone());

    let mut file = File::open(&path)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;

    // `print` (along with `to_string`/`hash_code`) is now a compiler builtin resolved during
    // code generation via the object protocol, so no source injection is needed.

    file_contents.insert(path_str.clone(), text.clone());

    let mut file_diagnostics = DiagnosticBag::new(Some(path_str.clone()));

    let lexer = Lexer::new(text);
    let mut parser = Parser::new(lexer, arena, &mut file_diagnostics);

    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(e) => {
            diagnostics.extend(&file_diagnostics);
            return Err(e);
        }
    };

    diagnostics.extend(&file_diagnostics);

    let program = ast.get_root();
    let parent_dir = path.parent().unwrap_or_else(|| Path::new(""));

    for import in &program.imports {
        let module_name = import.module_name.text.trim_matches('"');
        let mut import_path = parent_dir.join(module_name);
        if import_path.extension().is_none() {
            import_path.set_extension("ml");
        }

        let import_path_str = match import_path.to_str() {
            Some(s) => s.to_string(),
            None => {
                diagnostics.report_error(format!("Non-UTF-8 import path: {:?}", import_path), Some(import.module_name.position.clone()));
                continue;
            }
        };
        if !import_path.exists() {
            diagnostics.report_error(format!("Imported file not found: {}", import_path_str), Some(import.module_name.position.clone()));
            continue;
        }

        parse_file_recursive(&import_path_str, visited, all_functions, all_structs, all_enums, arena, diagnostics, file_contents)?;
    }

    // Tag every declaration with its source file so semantic diagnostics (which run on the
    // merged program) can report the correct file name.
    let file_tag: std::rc::Rc<str> = std::rc::Rc::from(path_str.as_str());
    for function in program.functions.iter().cloned() {
        let mut function = function;
        function.file_path = Some(file_tag.clone());
        all_functions.push(function);
    }
    for struct_decl in program.structs.iter().cloned() {
        let mut struct_decl = struct_decl;
        struct_decl.file_path = Some(file_tag.clone());
        for method in struct_decl.methods.iter_mut() {
            method.file_path = Some(file_tag.clone());
        }
        all_structs.push(struct_decl);
    }
    for enum_decl in program.enums.iter().cloned() {
        all_enums.push(enum_decl);
    }

    Ok(())
}
