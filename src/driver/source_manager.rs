use bumpalo::Bump;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Error, ErrorKind, Read};
use std::path::Path;

use crate::driver::diagnostics::DiagnosticBag;
use crate::syntax::lexer::Lexer;
use crate::syntax::parser::Parser;

/// Parses the embedded standard-collections prelude and merges its declarations into the
/// program. Uses the same arena as the user's files so all AST nodes share a lifetime.
pub fn merge_prelude<'a>(
    arena: &'a Bump,
    all_functions: &mut Vec<crate::syntax::nodes::FunctionNode<'a>>,
    all_structs: &mut Vec<crate::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
    all_extends: &mut Vec<crate::syntax::nodes::ExtendNode<'a>>,
    diagnostics: &mut DiagnosticBag,
    file_contents: &mut HashMap<String, String>,
) -> Result<(), Error> {
    // Each standard type lives in its own prelude file. The primitive files (int/char/string/...)
    // make the built-in types real, extensible classes via `extend` blocks. The list itself lives
    // in `crate::stdlib::PRELUDE_FILES` so the analyzer language service shares the same manifest.
    for &(prelude_name, prelude_src) in crate::stdlib::PRELUDE_FILES {
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
        for extend_decl in program.extends.iter().cloned() {
            let mut extend_decl = extend_decl;
            extend_decl.file_path = Some(file_tag.clone());
            for method in extend_decl.methods.iter_mut() {
                method.file_path = Some(file_tag.clone());
            }
            all_extends.push(extend_decl);
        }
    }

    Ok(())
}

/// Classifies a field's element type for JSON derivation. Returns the serialize/deserialize
/// expression templates, or `None` if the type is unsupported.
fn json_to_expr(elem_type: &str, access: &str, json_names: &HashSet<String>) -> Option<String> {
    Some(match elem_type {
        "int" => format!("JsonValue.from_int({})", access),
        "double" => format!("JsonValue.number({})", access),
        "float" => format!("JsonValue.number((double){})", access),
        "bool" => format!("JsonValue.boolean({})", access),
        "string" => format!("JsonValue.from_string({})", access),
        c if json_names.contains(c) => format!("{}.to_json()", access),
        _ => return None,
    })
}

fn json_from_expr(elem_type: &str, jexpr: &str, json_names: &HashSet<String>) -> Option<String> {
    Some(match elem_type {
        "int" => format!("{}.as_int()", jexpr),
        "double" => format!("{}.as_double()", jexpr),
        "float" => format!("(float){}.as_double()", jexpr),
        "bool" => format!("{}.as_bool()", jexpr),
        "string" => format!("{}.as_string()", jexpr),
        c if json_names.contains(c) => format!("{}.from_json({})", c, jexpr),
        _ => return None,
    })
}

/// Generates `extend <Class> { fun to_json(): JsonValue {...} static fun from_json(v): <Class> {...} }`
/// source for a single `@json` class, or `None` (after reporting a diagnostic) if a field type is
/// outside the supported set (primitives, `string`, other `@json` classes, and arrays of those).
fn generate_json_extend(
    struct_decl: &crate::syntax::nodes::struct_node::StructDeclarationNode,
    json_names: &HashSet<String>,
    diagnostics: &mut DiagnosticBag,
) -> Option<String> {
    let name = &struct_decl.name.text;
    let mut to_body = String::from("        let __o = JsonValue.dict();\n");
    let mut from_prelude = String::new();
    let mut from_fields: Vec<String> = Vec::new();

    for field in &struct_decl.fields {
        let fname = &field.name.text;
        let ftype = field.type_token.text.as_str();

        let mut json_key = fname.to_string();
        if let Some(prop_attr) = field
            .attributes
            .iter()
            .find(|a| a.name.text == "property_name")
        {
            if let Some(arg) = prop_attr.args.first() {
                json_key = arg.text.trim_matches('"').to_string();
            }
        }

        // Nullable field (`T?`): a JSON `null` maps to/from the Dream `null`, otherwise the inner
        // value is converted as usual. Only reference types can be nullable in Dream, so the inner
        // type is `string` or another `@json` class (nullable arrays are out of scope).
        if let Some(base) = ftype.strip_suffix('?') {
            let (to_inner, from_inner) = if base == "string" {
                (
                    format!("JsonValue.from_string(this.{f} ?? \"\")", f = fname),
                    format!("__src_{f}.as_string()", f = fname),
                )
            } else if json_names.contains(base) {
                (
                    format!("this.{f}.to_json()", f = fname),
                    format!("{c}.from_json(__src_{f})", c = base, f = fname),
                )
            } else {
                diagnostics.report_error(
                    format!("@json class '{}' field '{}' has unsupported nullable type '{}' (only `string?` and nullable @json classes are supported)", name, fname, ftype),
                    Some(field.name.position),
                );
                return None;
            };
            to_body.push_str(&format!(
                "        if (this.{f} == null) {{\n            __o.set(\"{k}\", JsonValue.none());\n        }} else {{\n            __o.set(\"{k}\", {to_inner});\n        }}\n",
                f = fname, k = json_key, to_inner = to_inner
            ));
            from_prelude.push_str(&format!(
                "        let __{f}: {ty} = null;\n        let __src_{f} = v.get(\"{k}\");\n        if (__src_{f}.is_null() == false) {{\n            __{f} = {from_inner};\n        }}\n",
                f = fname, k = json_key, ty = ftype, from_inner = from_inner
            ));
            from_fields.push(format!("__{f}", f = fname));
            continue;
        }

        if let Some(elem) = ftype.strip_suffix("[]") {
            // Array field: serialize/deserialize element-wise. Loop variables are suffixed with the
            // field name because Dream scopes locals per-function (not per-block).
            let to_elem = json_to_expr(elem, &format!("this.{}[__i_{}]", fname, fname), json_names);
            let from_elem = json_from_expr(
                elem,
                &format!("__src_{}.at(__i_{})", fname, fname),
                json_names,
            );
            match (to_elem, from_elem) {
                (Some(to_e), Some(from_e)) => {
                    to_body.push_str(&format!(
                        "        let __arr_{f} = JsonValue.array();\n        let __i_{f} = 0;\n        while (__i_{f} < this.{f}.len()) {{\n            __arr_{f}.push({to_e});\n            __i_{f} = __i_{f} + 1;\n        }}\n        __o.set(\"{k}\", __arr_{f});\n",
                        f = fname, k = json_key, to_e = to_e
                    ));
                    from_prelude.push_str(&format!(
                        "        let __src_{f} = v.get(\"{k}\");\n        let __{f} = Array.new<{elem}>(__src_{f}.size());\n        let __i_{f} = 0;\n        while (__i_{f} < __src_{f}.size()) {{\n            __{f}[__i_{f}] = {from_e};\n            __i_{f} = __i_{f} + 1;\n        }}\n",
                        f = fname, k = json_key, elem = elem, from_e = from_e
                    ));
                    from_fields.push(format!("__{f}", f = fname));
                }
                _ => {
                    diagnostics.report_error(
                        format!(
                            "@json class '{}' field '{}' has unsupported array element type '{}'",
                            name, fname, elem
                        ),
                        Some(field.name.position),
                    );
                    return None;
                }
            }
        } else {
            let to_e = json_to_expr(ftype, &format!("this.{}", fname), json_names);
            let from_e = json_from_expr(ftype, &format!("v.get(\"{}\")", json_key), json_names);
            match (to_e, from_e) {
                (Some(to_e), Some(from_e)) => {
                    to_body.push_str(&format!(
                        "        __o.set(\"{k}\", {to_e});\n",
                        k = json_key,
                        to_e = to_e
                    ));
                    from_fields.push(from_e);
                }
                _ => {
                    diagnostics.report_error(
                        format!(
                            "@json class '{}' field '{}' has unsupported type '{}'",
                            name, fname, ftype
                        ),
                        Some(field.name.position),
                    );
                    return None;
                }
            }
        }
    }
    to_body.push_str("        return __o;\n");

    let from_body = format!(
        "{prelude}        return {name}({fields});\n",
        prelude = from_prelude,
        name = name,
        fields = from_fields.join(", ")
    );

    Some(format!(
        "extend {name} {{\n    public fun to_json(): JsonValue {{\n{to_body}    }}\n    public static fun from_json(v: JsonValue): {name} {{\n{from_body}    }}\n}}\n",
        name = name, to_body = to_body, from_body = from_body
    ))
}

/// For every `@json` class, generates and parses its `to_json`/`from_json` converter `extend`
/// block and appends the methods to `all_extends`. Runs after all user/prelude declarations are
/// collected so cross-class (`@json` field) references resolve.
pub(crate) fn generate_json_derives<'a>(
    arena: &'a Bump,
    all_structs: &[crate::syntax::nodes::struct_node::StructDeclarationNode<'a>],
    all_extends: &mut Vec<crate::syntax::nodes::ExtendNode<'a>>,
    diagnostics: &mut DiagnosticBag,
    file_contents: &mut HashMap<String, String>,
) -> Result<(), Error> {
    let json_names: HashSet<String> = all_structs
        .iter()
        .filter(|s| s.attributes.iter().any(|a| a.name.text == "json"))
        .map(|s| s.name.text.clone())
        .collect();
    if json_names.is_empty() {
        return Ok(());
    }

    let mut source = String::new();
    for struct_decl in all_structs
        .iter()
        .filter(|s| s.attributes.iter().any(|a| a.name.text == "json"))
    {
        if struct_decl.generic_parameters.is_some() {
            diagnostics.report_error(
                format!(
                    "@json is not supported on generic class '{}'",
                    struct_decl.name.text
                ),
                Some(struct_decl.name.position),
            );
            continue;
        }
        if let Some(block) = generate_json_extend(struct_decl, &json_names, diagnostics) {
            source.push_str(&block);
            source.push('\n');
        }
    }

    if source.is_empty() {
        return Ok(());
    }

    let prelude_name = "<json-derive>".to_string();
    file_contents.insert(prelude_name.clone(), source.clone());
    let mut derive_diagnostics = DiagnosticBag::new(Some(prelude_name.clone()));
    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer, arena, &mut derive_diagnostics);
    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(e) => {
            diagnostics.extend(&derive_diagnostics);
            return Err(e);
        }
    };
    diagnostics.extend(&derive_diagnostics);

    let program = ast.get_root();
    let file_tag: std::rc::Rc<str> = std::rc::Rc::from(prelude_name.as_str());
    for extend_decl in program.extends.iter().cloned() {
        let mut extend_decl = extend_decl;
        extend_decl.file_path = Some(file_tag.clone());
        for method in extend_decl.methods.iter_mut() {
            method.file_path = Some(file_tag.clone());
        }
        all_extends.push(extend_decl);
    }
    Ok(())
}

#[derive(Default)]
pub struct ProgramAccumulator<'a> {
    pub visited: HashSet<String>,
    pub all_functions: Vec<crate::syntax::nodes::FunctionNode<'a>>,
    pub all_structs: Vec<crate::syntax::nodes::struct_node::StructDeclarationNode<'a>>,
    pub all_enums: Vec<crate::syntax::nodes::EnumDeclarationNode>,
    pub all_extends: Vec<crate::syntax::nodes::ExtendNode<'a>>,
    pub all_globals: Vec<crate::syntax::nodes::GlobalVariableNode<'a>>,
    pub file_contents: HashMap<String, String>,
}

pub fn resolve_import_path(base_dir: &Path, module_name: &str) -> std::path::PathBuf {
    let mut import_path = base_dir.join(module_name);
    if import_path.extension().is_none() {
        import_path.set_extension("dream");
    }
    import_path
}

/// Recursively parses `file_path` and every file it imports, merging all declarations into the
/// `all_*` accumulators. Each declaration is tagged with its originating file so semantic
/// diagnostics (which run on the merged program) can attribute errors correctly.
pub fn parse_file_recursive<'a>(
    file_path: &String,
    acc: &mut ProgramAccumulator<'a>,
    arena: &'a Bump,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), Error> {
    let path = Path::new(file_path).canonicalize()?;
    let path_str = path
        .to_str()
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("Non-UTF-8 file path: {:?}", path),
            )
        })?
        .to_string();

    if acc.visited.contains(&path_str) {
        return Ok(()); // Already processed
    }
    acc.visited.insert(path_str.clone());

    let mut file = File::open(&path)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;

    // `print` (along with `to_string`/`hash_code`) is now a compiler builtin resolved during
    // code generation via the object protocol, so no source injection is needed.

    acc.file_contents.insert(path_str.clone(), text.clone());

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
        let import_path = resolve_import_path(parent_dir, module_name);

        let import_path_str = match import_path.to_str() {
            Some(s) => s.to_string(),
            None => {
                diagnostics.report_error(
                    format!("Non-UTF-8 import path: {:?}", import_path),
                    Some(import.module_name.position),
                );
                continue;
            }
        };
        if !import_path.exists() {
            diagnostics.report_error(
                format!("Imported file not found: {}", import_path_str),
                Some(import.module_name.position),
            );
            continue;
        }

        parse_file_recursive(&import_path_str, acc, arena, diagnostics)?;
    }

    // Tag every declaration with its source file so semantic diagnostics (which run on the
    // merged program) can report the correct file name.
    let file_tag: std::rc::Rc<str> = std::rc::Rc::from(path_str.as_str());
    for function in program.functions.iter().cloned() {
        let mut function = function;
        function.file_path = Some(file_tag.clone());
        acc.all_functions.push(function);
    }
    for struct_decl in program.structs.iter().cloned() {
        let mut struct_decl = struct_decl;
        struct_decl.file_path = Some(file_tag.clone());
        for method in struct_decl.methods.iter_mut() {
            method.file_path = Some(file_tag.clone());
        }
        acc.all_structs.push(struct_decl);
    }
    for enum_decl in program.enums.iter().cloned() {
        acc.all_enums.push(enum_decl);
    }
    for extend_decl in program.extends.iter().cloned() {
        let mut extend_decl = extend_decl;
        extend_decl.file_path = Some(file_tag.clone());
        for method in extend_decl.methods.iter_mut() {
            method.file_path = Some(file_tag.clone());
        }
        acc.all_extends.push(extend_decl);
    }
    for global in program.globals.iter().cloned() {
        let mut global = global;
        global.file_path = Some(file_tag.clone());
        acc.all_globals.push(global);
    }

    Ok(())
}
