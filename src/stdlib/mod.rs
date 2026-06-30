use crate::syntax::nodes::Type;

/// The embedded standard-library prelude, in the exact order it must be merged. This is the
/// single source of truth shared by the compiler's source manager and the `dream-analyzer`
/// language service, so the two can never drift. The primitive files (int/char/string/...)
/// only attach methods to built-in types, so their relative order does not matter.
pub const PRELUDE_FILES: &[(&str, &str)] = &[
    ("<std>/core.dream", include_str!("core.dream")),
    ("<std>/option.dream", include_str!("option.dream")),
    ("<std>/result.dream", include_str!("result.dream")),
    ("<std>/list.dream", include_str!("list.dream")),
    ("<std>/map.dream", include_str!("map.dream")),
    ("<std>/int.dream", include_str!("int.dream")),
    ("<std>/long.dream", include_str!("long.dream")),
    ("<std>/uint.dream", include_str!("uint.dream")),
    ("<std>/ulong.dream", include_str!("ulong.dream")),
    ("<std>/byte.dream", include_str!("byte.dream")),
    ("<std>/char.dream", include_str!("char.dream")),
    ("<std>/string.dream", include_str!("string.dream")),
    ("<std>/bool.dream", include_str!("bool.dream")),
    ("<std>/float.dream", include_str!("float.dream")),
    ("<std>/double.dream", include_str!("double.dream")),
    ("<std>/jsref.dream", include_str!("jsref.dream")),
    ("<std>/json.dream", include_str!("json.dream")),
    ("<std>/math.dream", include_str!("math.dream")),
    ("<std>/regex.dream", include_str!("regex.dream")),
    ("<std>/http.dream", include_str!("http.dream")),
    ("<std>/file.dream", include_str!("file.dream")),
    ("<std>/system.dream", include_str!("system.dream")),
    ("<std>/promise.dream", include_str!("promise.dream")),
];

pub struct StdlibFunction {
    pub name: String,
    pub parameters: Vec<String>,
    pub return_type: Option<Type>,
    /// When `true`, codegen emits this function's body inline (see `RUNTIME_STRINGS` / the object
    /// runtime) instead of importing it from the host. This is the single source of truth for the
    /// import-vs-inline decision; the module import emitter consults it rather than a parallel list.
    pub inline: bool,
}

impl StdlibFunction {
    /// A host-imported stdlib function (lowered to a WASM `(import "env" ...)`).
    fn imported(name: &str, parameters: &[&str], return_type: Option<Type>) -> Self {
        Self {
            name: name.to_string(),
            parameters: parameters.iter().map(|s| s.to_string()).collect(),
            return_type,
            inline: false,
        }
    }

    /// Host functions that are always imported into every module but are NOT user-callable.
    /// The `print`/`println` builtins lower to these; users never name them directly.
    pub fn host_imports() -> Vec<StdlibFunction> {
        let imports = vec![
            Self::imported("print_string", &["string"], None),
            Self::imported("print_int", &["int"], None),
            Self::imported("print_float", &["float"], None),
            Self::imported("print_double", &["double"], None),
            Self::imported("print_char", &["char"], None),
        ];
        imports
    }

    /// User-callable stdlib *free* functions registered in the function table. There are none: the
    /// former string/array/debug primitives are now class members (`String.alloc`/`String.set`,
    /// `Array.new`, `Debug.free_list_head`) or builtin pseudo-methods (`s.char_at(i)`), lowered by
    /// the compiler to their `RUNTIME_STRINGS` helpers (`$string_alloc`/`$char_at`/...). The runtime
    /// bodies themselves are still emitted unconditionally from `RUNTIME_STRINGS`.
    pub fn get_all() -> Vec<StdlibFunction> {
        vec![]
    }
}
