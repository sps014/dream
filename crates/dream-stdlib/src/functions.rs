use dream_syntax::nodes::Type;

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

    /// User-callable stdlib *free* functions.
    pub fn get_all() -> Vec<StdlibFunction> {
        vec![]
    }
}
