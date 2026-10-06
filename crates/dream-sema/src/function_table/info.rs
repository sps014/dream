use super::FunctionIdentity;
use dream_syntax::nodes::{FunctionNode, Type, Visibility};
use dream_types::{TypeCtx, TypeId};

#[derive(Debug, Clone)]
pub struct FunctionTableInfo {
    pub identity: FunctionIdentity,
    pub name: String,
    pub return_type: Option<Type>,
    pub resolved_return: TypeId,
    pub parameters: Vec<TypeId>,
    /// Source annotations retained for contextual typing of defaults and lambdas.
    pub parameter_types: Vec<Type>,
    /// Per-parameter declared names, parallel to `parameters`, used to resolve named arguments
    /// (`f(name: value)`) at call sites back to a positional index. Empty for entries with no
    /// source-level parameter names (synthesized/stdlib entries built via
    /// [`FunctionTableInfo::new`]) — a named-argument call to one of those is rejected with a clear
    /// diagnostic rather than silently misresolving.
    pub param_names: Vec<String>,
    /// True when the last declared parameter is `...name: T[]` (variadic): a call may supply zero
    /// or more trailing arguments of the array's element type in that slot, which the analyzer
    /// collects into an array before argument type-checking. `false` for every synthesized/stdlib
    /// entry and every declaration with no variadic parameter.
    pub is_variadic: bool,
    /// Per-parameter `ref` flag, parallel to `parameters`: true when the declaration is `ref
    /// name: T`, requiring the call site to pass a matching `ref` argument (see
    /// `Analyzer::analyze_ref_argument`). Always all-`false` for synthesized/stdlib entries.
    pub is_ref: Vec<bool>,
    /// Per-parameter sink (`take`) flag, parallel to `parameters`: true when the parameter is
    /// unmarked (neither `ref` nor `borrow`) and not the synthetic `this` receiver — the callee
    /// takes ownership of the caller's +1. Explicit `borrow` is share ABI and is `false` here.
    pub is_take: Vec<bool>,
    /// Per-parameter constant-literal default values, parallel to `parameters`. `None` means the
    /// parameter is required. Defaults are always trailing (enforced by the parser), so a call may
    /// omit the trailing defaulted arguments and the analyzer substitutes these literals.
    pub defaults: Vec<Option<Type>>,
    /// True when the declaration is `async fun`: calling it eagerly starts a task and yields
    /// `Future<T>` (where `T` is `return_type`). Awaiting a call to it produces `T`.
    pub is_async: bool,
    /// True when the declaration is a `static fun` method (no implicit `this`, dispatched as
    /// `Type.method(...)`). Used by the indexer/enumerator sugar sites to reject static methods as
    /// `[]`/`for..in` hooks. Always `false` for free functions and synthesized/stdlib entries.
    pub is_static: bool,
    /// True when the declaration carries `@unsafe`: it performs a manual-memory-management
    /// operation (raw `Pointer<T>` alloc/free/realloc/read/write) with no compiler-enforced safety
    /// net. Calling it is only permitted from another `@unsafe` function/method — checked at every
    /// call site (see `Analyzer::check_unsafe_call`, `src/semantics/analyzer/calls/mod.rs`).
    pub is_unsafe: bool,
    /// Runtimes this declaration is available on (`@native`/`@node`/`@web`). Absent all three
    /// means every runtime; checked at call sites via `Analyzer::check_runtime_call`.
    pub runtime_support: dream_abi::attributes::RuntimeSupport,
    pub intrinsic_name: Option<String>,
    /// Accessibility of the declaration. For methods this gates external calls (private methods
    /// may only be called from within their declaring type; `internal` ones from anywhere in the
    /// same module). Defaults to `Public` for synthesized/stdlib entries so they are callable
    /// everywhere.
    pub visibility: Visibility,
    /// Source file the declaration came from, used for file/module-level visibility: a non-public
    /// declaration is only reachable from its own file. `None` for synthesized/stdlib entries,
    /// which are always visible.
    pub declaring_file: Option<std::rc::Rc<str>>,
    /// The declaring file's `module a.b.c;` path, if any — `None` for a file with no `module`
    /// declaration (the implicit root module) as well as for synthesized/stdlib entries. Set by
    /// the analyzer's registration pass (`FunctionTableInfo::from` cannot see the file/module map
    /// on its own); drives the cross-module duplicate-name resolution in [`FunctionTable::add_overload`].
    pub declaring_module: Option<std::rc::Rc<str>>,
}

impl FunctionTableInfo {
    pub fn new(
        name: String,
        return_type: Option<Type>,
        parameters: Vec<TypeId>,
        identity: FunctionIdentity,
        resolved_return: TypeId,
    ) -> FunctionTableInfo {
        let defaults = vec![None; parameters.len()];
        let is_ref = vec![false; parameters.len()];
        let is_take = vec![false; parameters.len()];
        let param_names = Vec::new();
        FunctionTableInfo {
            identity,
            name,
            return_type,
            resolved_return,
            parameters,
            parameter_types: Vec::new(),
            param_names,
            is_variadic: false,
            is_ref,
            is_take,
            defaults,
            is_async: false,
            is_static: false,
            is_unsafe: false,
            runtime_support: dream_abi::attributes::RuntimeSupport::ALL,
            intrinsic_name: None,
            visibility: Visibility::Public,
            declaring_file: None,
            declaring_module: None,
        }
    }
    pub fn from(func: &FunctionNode, type_ctx: &mut TypeCtx) -> Self {
        let parameters: Vec<_> = func
            .parameters
            .iter()
            .map(|p| type_ctx.lower(&p.type_))
            .collect();
        let def = type_ctx.register_function(&func.name.text, &parameters);
        Self::from_identity(func, (def, Vec::new()), type_ctx)
    }

    pub fn from_identity(
        func: &FunctionNode,
        identity: FunctionIdentity,
        type_ctx: &mut TypeCtx,
    ) -> Self {
        let name = func.name.clone();
        let return_type = func.return_type.clone();
        let mut parameters: Vec<TypeId> = vec![];
        let mut param_names: Vec<String> = vec![];
        let mut defaults: Vec<Option<Type>> = vec![];
        let mut is_ref: Vec<bool> = vec![];
        let mut is_take: Vec<bool> = vec![];
        // A host function cannot release a Dream reference, so the sink default is backwards for an
        // `extern`: it makes the call site retain an argument nothing will ever drop. Externs pass at
        // +0 unless the declaration opts in with `@consuming` (`Buffer.free`, which really does take
        // the array).
        let host_borrows = func.is_extern && !dream_abi::attributes::is_consuming(&func.attributes);
        for i in func.parameters.iter() {
            let j = i.clone();
            parameters.push(type_ctx.lower(&j.type_));
            defaults.push(j.default);
            is_ref.push(j.is_ref);
            is_take.push(!j.is_ref && !j.is_borrow && j.name.text != "this" && !host_borrows);
            param_names.push(j.name.text);
        }
        let intrinsic_name = dream_abi::intrinsics::intrinsic_key(&func.attributes);
        let is_variadic = func
            .parameters
            .last()
            .map(|p| p.is_variadic)
            .unwrap_or(false);
        let resolved_return = return_type
            .as_ref()
            .map(|ret| type_ctx.lower(ret))
            .unwrap_or_else(|| type_ctx.interner.void());
        let mut info = FunctionTableInfo::new(
            name.text,
            return_type,
            parameters,
            identity,
            resolved_return,
        );
        info.parameter_types = info
            .parameters
            .iter()
            .map(|&ty| type_ctx.syntax_type(ty))
            .collect();
        info.return_type = func
            .return_type
            .as_ref()
            .map(|_| type_ctx.syntax_type(resolved_return));
        info.param_names = param_names;
        info.is_variadic = is_variadic;
        info.is_ref = is_ref;
        info.is_take = is_take;
        info.defaults = defaults;
        info.is_async = func.is_async;
        info.is_static = func.is_static;
        info.is_unsafe = func.attributes.iter().any(|a| a.name.text == "unsafe");
        info.runtime_support =
            dream_abi::attributes::RuntimeSupport::from_attributes(&func.attributes);
        info.intrinsic_name = intrinsic_name;
        // `extern` functions/methods are interop entry points (WASM imports): they cannot be
        // host-exported and privacy is meaningless for them, so they are always call-visible.
        info.visibility = if func.is_extern {
            Visibility::Public
        } else {
            func.visibility
        };
        info.declaring_file = func.file_path.clone();
        info
    }

    /// The number of leading required parameters: the index of the first parameter that has a
    /// default value, or the full parameter count when none do. A call must supply at least this
    /// many arguments; the remaining trailing parameters may be omitted (their defaults are used).
    pub fn required_params(&self) -> usize {
        self.defaults
            .iter()
            .position(|d| d.is_some())
            .unwrap_or(self.parameters.len())
    }

    /// True if any parameter carries a default value.
    pub fn has_defaults(&self) -> bool {
        self.defaults.iter().any(|d| d.is_some())
    }
}
