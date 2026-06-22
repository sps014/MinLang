use std::collections::HashMap;
use crate::semantics::errors::SymbolError;
use crate::syntax::nodes::{FunctionNode, Type};
use crate::syntax::nodes::types::strip_nullable;
use crate::stdlib::StdlibFunction;

/// Whether an argument of type `arg` may bind to a parameter of type `param` for the purpose of
/// overload *viability* (exactness is scored separately by the caller). Mirrors the implicit
/// conversions the type checker already permits: `object` widening, enum<->int, numeric widening
/// among int/float/double, and matching base types across a nullable marker. `is_enum` reports
/// whether a type name denotes a registered enum (supplied by each caller's own tables).
pub fn overload_arg_compatible(param: &str, arg: &str, is_enum: impl Fn(&str) -> bool) -> bool {
    if param == arg {
        return true;
    }
    if param == "object" {
        return true;
    }
    if (is_enum(param) && arg == "int") || (is_enum(arg) && param == "int") {
        return true;
    }
    let numeric = |t: &str| matches!(t, "int" | "float" | "double");
    if numeric(param) && numeric(arg) {
        return true;
    }
    strip_nullable(param) == strip_nullable(arg)
}

#[derive(Debug, Clone)]
pub struct FunctionTable {
    pub functions: HashMap<String, FunctionTableInfo>,
    /// Base name -> the emitted keys of every overload registered under it, in declaration
    /// order. A base with a single entry keeps its bare name; a base with 2+ entries has each
    /// overload stored under a signature-mangled key (see [`overload_key`]).
    pub overloads: HashMap<String, Vec<String>>,
}

/// Result of resolving an overloaded call against the argument types present at a call site.
pub enum OverloadResolution {
    Unique(String),
    None,
    Ambiguous(Vec<String>),
}

/// Builds the signature-mangled emitted name for one overload: the base name followed by each
/// parameter type, joined with `.` — a valid WAT identifier character, distinct from the `_`
/// used by generic monomorphization so the two schemes never collide. E.g. base `add` with
/// `[int, int]` becomes `add.int.int`; a zero-parameter overload becomes `add.`.
pub fn overload_key(base: &str, parameters: &[String]) -> String {
    let mut key = String::from(base);
    key.push('.');
    key.push_str(&parameters.join("."));
    key
}


impl FunctionTable {
    pub fn new() -> FunctionTable {
        let mut table = FunctionTable {
            functions: HashMap::new(),
            overloads: HashMap::new(),
        };
        
        for std_func in StdlibFunction::get_all() {
            let info = FunctionTableInfo::new(
                std_func.name.clone(),
                std_func.return_type,
                std_func.parameters,
            );
            table.functions.insert(std_func.name, info);
        }
        
        table
    }

    pub fn add_function(&mut self, name: String, function_info: FunctionTableInfo) -> Result<(), SymbolError>
    {
        if self.functions.contains_key(&name)
        {
            return Err(SymbolError::new(format!("Function already exists ({})", name)));
        }
        self.functions.insert(name, function_info);
        Ok(())
    }

    /// Registers one (possibly overloaded) declaration under `base`. The first declaration of a
    /// base keeps the bare name; when a second declaration arrives the original is *promoted* to
    /// its signature-mangled key and the new one is mangled too, so non-overloaded code keeps its
    /// original emitted names. Returns the emitted key chosen for `info`, or an error if an
    /// identical signature was already registered under `base`.
    pub fn add_overload(&mut self, base: &str, mut info: FunctionTableInfo) -> Result<String, SymbolError> {
        let existing = self.overloads.entry(base.to_string()).or_default();
        if existing.is_empty() {
            if self.functions.contains_key(base) {
                return Err(SymbolError::new(format!("Function already exists ({})", base)));
            }
            info.name = base.to_string();
            existing.push(base.to_string());
            self.functions.insert(base.to_string(), info);
            return Ok(base.to_string());
        }
        // Promote a lone bare singleton to its mangled key the moment a second overload appears.
        if existing.len() == 1 && existing[0] == base {
            if let Some(mut first) = self.functions.remove(base) {
                let first_key = overload_key(base, &first.parameters);
                first.name = first_key.clone();
                self.functions.insert(first_key.clone(), first);
                existing[0] = first_key;
            }
        }
        let key = overload_key(base, &info.parameters);
        if self.functions.contains_key(&key) {
            return Err(SymbolError::new(format!(
                "Duplicate overload: '{}' with the same parameter types is already defined", base
            )));
        }
        info.name = key.clone();
        existing.push(key.clone());
        self.functions.insert(key.clone(), info);
        Ok(key)
    }

    /// Whether `base` has more than one overload (i.e. its declarations are signature-mangled).
    pub fn is_overloaded(&self, base: &str) -> bool {
        self.overloads.get(base).map(|v| v.len() > 1).unwrap_or(false)
    }

    /// The emitted name of the declaration of `base` whose parameter list is `parameters`: the
    /// bare base when `base` is not overloaded, otherwise the signature-mangled key.
    pub fn resolve_emitted_name(&self, base: &str, parameters: &[String]) -> String {
        if self.is_overloaded(base) {
            overload_key(base, parameters)
        } else {
            base.to_string()
        }
    }

    /// Selects the overload of `base` that best matches `args`. Exact type matches are preferred;
    /// `compat` supplies the fallback compatibility (object widening, enum/int, numeric, nullable).
    /// A single best candidate wins; ties yield `Ambiguous`; no viable candidate yields `None`.
    /// When `base` is not an overload set, falls back to the plain function keyed by `base`.
    pub fn select_overload(&self, base: &str, args: &[String], compat: impl Fn(&str, &str) -> bool) -> OverloadResolution {
        let keys = match self.overloads.get(base) {
            Some(keys) => keys,
            None => {
                return if self.functions.contains_key(base) {
                    OverloadResolution::Unique(base.to_string())
                } else {
                    OverloadResolution::None
                };
            }
        };
        let mut scored: Vec<(i32, &String)> = Vec::new();
        for key in keys {
            let info = match self.functions.get(key) {
                Some(info) => info,
                None => continue,
            };
            if info.parameters.len() != args.len() {
                continue;
            }
            let mut score = 0i32;
            let mut viable = true;
            for (param, arg) in info.parameters.iter().zip(args.iter()) {
                if param == arg {
                    score += 1;
                } else if compat(param, arg) {
                    // Viable via fallback (e.g. object widening); contributes no exactness score.
                } else {
                    viable = false;
                    break;
                }
            }
            if viable {
                scored.push((score, key));
            }
        }
        let max_score = match scored.iter().map(|(s, _)| *s).max() {
            Some(max) => max,
            None => return OverloadResolution::None,
        };
        let best: Vec<String> = scored.iter()
            .filter(|(s, _)| *s == max_score)
            .map(|(_, k)| (*k).clone())
            .collect();
        if best.len() == 1 {
            OverloadResolution::Unique(best.into_iter().next().unwrap())
        } else {
            OverloadResolution::Ambiguous(best)
        }
    }


    pub fn get_function(&self, name: &String) -> Result<FunctionTableInfo, SymbolError> {
        if !self.functions.contains_key(name)
        {
            return Err(SymbolError::new(format!("Function does not exist ({})", name)));
        }
        Ok(self.functions.get(name).unwrap().clone())
    }
}


#[derive(Debug,Clone)]
pub  struct FunctionTableInfo
{
    #[allow(dead_code)]
    pub name: String,
    pub return_type: Option<Type>,
    pub parameters: Vec<String>,
}

impl FunctionTableInfo {
    pub fn new(name: String, return_type: Option<Type>, parameters: Vec<String>) -> FunctionTableInfo {
        FunctionTableInfo {
            name,
            return_type,
            parameters,
        }
    }
    pub fn from(func:&FunctionNode)->Self
    {
        let name = func.name.clone();
        let return_type = func.return_type.clone();
        let mut parameters:Vec<String> = vec![];
        for i in func.parameters.iter()
        {
            let j=i.clone();
            parameters.push(j.type_.get_type());
        }
        FunctionTableInfo::new(name.text, return_type, parameters)
    }
}
