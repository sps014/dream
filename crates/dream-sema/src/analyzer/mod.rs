use crate::errors::SemanticError;
use crate::function_table::FunctionTable;
use crate::module_graph::{ModuleGraph, ProgramView};
use crate::struct_table::StructTable;
use crate::symbol_table::SymbolTable;
use crate::union_table::UnionTable;
use bumpalo::Bump;
use dream_abi::attributes::{CompileTargets, RuntimeSupport};
use dream_diagnostics::{Diagnostic, DiagnosticBag};
use dream_syntax::nodes::types::FUTURE_TYPE;
use dream_syntax::nodes::{EnumDeclarationNode, ExtendNode};
use dream_syntax::nodes::{ExpressionNode, FunctionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use dream_text::line_text::LineText;
use dream_text::text_span::TextSpan;
use dream_types::{DefKind, TypeCtx};
use indexmap::IndexMap;
use indexmap::{IndexMap as HashMap, IndexSet as HashSet};
use std::cell::RefCell;
use std::rc::Rc;

mod await_rules;
mod borrow_check;
mod calls;
mod closure_cycles;
mod declarations;
mod expressions;
mod generics;
mod hir_emit;
pub mod ide;
mod js_interop;
mod ownership;
mod receiver_modes;
mod statements;
mod switch_unions;
mod type_checker;

pub use ide::{IdeSnapshot, TypeSummary};

mod source;
use source::file_path_string;

mod diagnostics;
use diagnostics::{report, report_noted, report_with_code, report_with_notes};

/// An empty source span, used for diagnostics on synthesized nodes that have no real
/// position in the user's source (e.g. array element type mismatches).
pub(in crate::analyzer) fn empty_span() -> TextSpan {
    TextSpan::new((0, 0), &Rc::new(LineText::new(String::new())))
}

/// Best-effort 1-based source line of a statement, used to place debug-info line markers. Picks a
/// representative token/expression for each statement kind; returns `None` for statements with no
/// anchoring token (bare `break`/`continue`, `return;`), which simply carry no breakpoint line.
pub(super) fn statement_line(statement: &dream_syntax::nodes::StatementNode) -> Option<usize> {
    use dream_syntax::nodes::StatementNode;
    let line = |span: Option<TextSpan>| span.map(|s| s.line_no);
    match statement {
        StatementNode::Assignment(tok, _)
        | StatementNode::Declaration(tok, _, _, _)
        | StatementNode::FunctionInvocation(tok, _, _)
        | StatementNode::MethodInvocation(_, tok, _, _)
        | StatementNode::MemberAssignment(_, tok, _)
        | StatementNode::ForEach(tok, _, _, _, _) => Some(tok.position.line_no),
        StatementNode::TupleDeclaration { pattern, init, .. } => pattern
            .position()
            .map(|s| s.line_no)
            .or_else(|| line(init.position())),
        StatementNode::IndexAssignment(arr, _, _) => line(arr.position()),
        StatementNode::Return(Some(e))
        | StatementNode::ExpressionStatement(e)
        | StatementNode::AwaitStmt(e)
        | StatementNode::While(e, _)
        | StatementNode::DoWhile(_, e)
        | StatementNode::Lock(e, _)
        | StatementNode::IfElse(e, _, _, _)
        | StatementNode::Switch(e, _, _) => line(e.position()),
        StatementNode::Defer(Some(e), _) => line(e.position()),
        StatementNode::Defer(None, _) | StatementNode::Overflow(..) => None,
        StatementNode::For(_, Some(cond), _, _) => line(cond.position()),
        StatementNode::Labeled(_, inner) => statement_line(inner),
        StatementNode::Return(None)
        | StatementNode::For(_, None, _, _)
        | StatementNode::Break(_)
        | StatementNode::Continue(_) => None,
    }
}

/// Creates a token with an empty source span, used when the analyzer synthesizes
/// AST nodes (injected `this` parameters, monomorphized generic types, etc.).
pub(in crate::analyzer) fn synthetic_token(kind: TokenKind, text: &str) -> SyntaxToken {
    SyntaxToken::new(kind, empty_span(), text.to_string())
}

mod generic_types;
pub use generic_types::{generic_bindings, substitute_generic_type};
use generic_types::{generic_param_names, lookup_binding};

/// The `$` keeps synthesized property getters distinct from source identifiers.
pub fn getter_member_name(prop: &str) -> String {
    format!("get${}", prop)
}

/// The internal member name a property setter is registered under (see [`getter_member_name`]).
pub fn setter_member_name(prop: &str) -> String {
    format!("set${}", prop)
}

/// The internal member name a class member is registered under: the `$`-tagged accessor name for a
/// property `get`/`set`, or the plain method/field name otherwise.
pub fn accessor_member_name(method: &FunctionNode) -> String {
    match method.accessor {
        Some(dream_syntax::nodes::function::AccessorKind::Get) => {
            getter_member_name(&method.name.text)
        }
        Some(dream_syntax::nodes::function::AccessorKind::Set) => {
            setter_member_name(&method.name.text)
        }
        None => method.name.text.clone(),
    }
}

/// Maps each generic parameter name to the concrete `Type` bound to it for one monomorphization.
/// Insertion-ordered so the mangled instance symbol (built from the values in order) is
/// deterministic. Stores the structured AST `Type` (not a stringified name), so the monomorphizer
/// substitutes and lowers it directly rather than round-tripping through `get_type()`/reparse.
pub type GenericBindings = IndexMap<String, Type>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GenericExtendTarget {
    Nominal(dream_types::DefId),
    Array,
}

/// Enum name -> (member name -> integer value). Insertion-ordered at both levels so the enum
/// variant-name interning that feeds emitted output happens in a deterministic (declaration) order.
pub type EnumTable = IndexMap<dream_types::DefId, IndexMap<String, i32>>;

/// A resolved top-level variable, carried from semantic analysis into code generation so the
/// generator can emit the corresponding WASM global and the module-init store (and decide whether
/// to export it to the host).
#[derive(Debug, Clone)]
pub struct GlobalSymbol {
    pub name: String,
    pub ty: dream_types::TypeId,
    pub is_const: bool,
    pub visibility: dream_syntax::nodes::Visibility,
    /// Source file this global was declared in, for file/module-level visibility. `None` for
    /// synthesized globals (always visible).
    pub file_path: Option<Rc<str>>,
}

pub struct SemanticInfo<'a> {
    pub function_table: &'a FunctionTable,
    pub struct_table: &'a StructTable,
    pub instantiated_generics:
        IndexMap<crate::function_table::FunctionIdentity, (GenericBindings, &'a FunctionNode<'a>)>,
    pub struct_methods: Vec<(&'a FunctionNode<'a>, GenericBindings)>,
    pub enums: EnumTable,
    /// Layout of every (monomorphized) discriminated union, surfaced to codegen so it can
    /// allocate variant blocks, lower `match`, and emit discriminant-aware releases.
    pub unions: UnionTable,
    pub globals: Vec<GlobalSymbol>,
    /// The typed, name-resolved HIR emitted alongside analysis. It is the sole input the MIR backend
    /// consumes; a function whose every construct is representable is emitted here (all others are
    /// skipped and produce no backend output).
    pub hir: dream_hir::Hir,
}

/// Groups context arguments frequently passed together to simplify function signatures.
pub struct AnalyzerContext<'a, 'b> {
    pub parent_function: &'b FunctionNode<'a>,
    pub symbol_table: &'b Rc<RefCell<SymbolTable>>,
}

/// Outcome of resolving `obj.member` as a struct field, shared by member reads (`obj.m`) and writes
/// (`obj.m = v`) via [`Analyzer::resolve_member_field`]. Callers apply their own error-reporting and
/// accessor (getter/setter) policy to the non-`Field` variants, which differs between read and write
/// positions.
pub(super) enum MemberField {
    /// `member` is a declared field of the (possibly monomorphized) `struct_ty`. Any "private
    /// field" diagnostic has already been reported.
    Field {
        struct_ty: dream_types::TypeId,
        field_type: Type,
    },
    /// The receiver's type is not a class/struct.
    NotAStruct,
    /// The receiver is a struct instance whose table entry is missing.
    StructNotFound { struct_name: String },
    /// `member` is not a declared field of `struct_name` (the caller may still resolve it as a
    /// getter/setter accessor).
    NotAField {
        struct_name: String,
        struct_ty: dream_types::TypeId,
    },
}

pub struct Analyzer<'a> {
    graph: &'a ModuleGraph<'a>,
    program: &'a ProgramView<'a>,
    function_table: FunctionTable,
    struct_table: StructTable,
    arena: &'a Bump,
    generic_functions: IndexMap<dream_types::DefId, &'a FunctionNode<'a>>,
    instantiated_generics:
        IndexMap<crate::function_table::FunctionIdentity, (GenericBindings, &'a FunctionNode<'a>)>,
    /// Arrow-lambdas (capturing or not) lowered to synthesized top-level functions (`__lambda_0`,
    /// ...), keyed by their synthesized name, paired with the generic bindings active at the
    /// lambda literal's own use site (e.g. `TOut` -> `int` for a lambda written inside a
    /// `Task.spawn<TOut>` method) so its body is re-checked under the same substitution when
    /// analyzed. Bodies are analyzed in the same deferred fixpoint pass as `instantiated_generics`
    /// (see `analyze_pending_instantiations`), since a function's body cannot be analyzed while
    /// another function's analysis is already in progress. The lambda literal itself is never
    /// generic in v1 - only the *enclosing* context it was written in can be.
    pending_lambdas: IndexMap<dream_types::DefId, (&'a FunctionNode<'a>, GenericBindings)>,
    /// Counter used to name synthesized lambda functions uniquely (`__lambda_<n>`).
    lambda_counter: usize,
    /// Names, local to the function currently being analyzed, that a nested lambda captures — and
    /// so must be boxed into a `CaptureCell<T>` rather than stored as a plain local (see
    /// `expressions::capture_scan::scan_function_captures`, run once as a pre-pass before the
    /// function's body is analyzed). Cleared and repopulated per function in `hir_begin_function`.
    boxed_locals: HashSet<String>,
    /// Names that are `ref`-passed somewhere in the current function's body
    /// (`expressions::capture_scan::scan_ref_argument_targets`) but are *not* in `boxed_locals` —
    /// i.e. never closure-captured. These are boxed into the stack-resident `RefBox<T>` value
    /// struct instead of the heap `CaptureCell<T>` (see `hir_declare_local`/`hir_begin_function`).
    /// Cleared and repopulated per function in `hir_begin_function`.
    ref_boxed_locals: HashSet<String>,
    /// For each synthesized capturing-lambda function (keyed by its lifted name, e.g. `__lambda_3`):
    /// the ordered list of `(captured name, its type in the enclosing scope)` it closes over.
    /// Consulted by identifier resolution *inside that lifted function's own body* to redirect a
    /// captured name's reads/writes through `env.<field>.value` instead of a plain local (see
    /// `identifiers::resolve_identifier`/`bindings::analyze_assignment`), and by
    /// `expressions::lambda` to build the matching `Closure_env_<n>` class + construction site.
    closure_captures: HashMap<dream_types::DefId, Vec<(String, Type)>>,
    /// Resolved receiver modes (concrete owner and method slot -> Borrow/Unique) from the receiver-
    /// exclusivity pass. Populated by `classify_receiver_modes` after body analysis on clean
    /// programs; consulted by dispatch metadata and borrow-collision checking.
    pub(in crate::analyzer) receiver_modes:
        HashMap<(dream_types::TypeId, usize), dream_syntax::nodes::function::ReceiverMode>,
    /// Fun-typed locals whose initializer/last assignment was a *capturing* `fun(...)` value
    /// (`true`) or a known captureless one (`false`). Used at the JS boundary to reject stashed
    /// capturing lambdas (`let h: fun(js): void = (e) => { use(x); }; el.addEventListener(..., h)`)
    /// after the construction site is no longer visible in the HExpr. Cleared per function in
    /// `hir_begin_function`. Params are intentionally absent (higher-order wrappers stay allowed).
    capturing_fun_locals: HashMap<String, bool>,
    /// Stack of `is`-with-binding aliases visible while analyzing a later conjunct of the same
    /// top-level `&&` chain (`if (x is T t && t.ok())`): each entry is `(bound name, target type,
    /// original operand expression)`. Pushed by `analyze_binary_expression` before analyzing the
    /// right operand of `&&` when the left operand collects one or more `is`-bindings, popped
    /// immediately after. Consulted by identifier resolution ahead of the symbol table, so a
    /// reference to the bound name resolves to a fresh `(T)operand` cast rather than a real local
    /// — this is analysis-only sugar; the branch body's own binding is still a real local declared
    /// by `declare_is_bindings`.
    is_binding_aliases: Vec<(String, Type, &'a ExpressionNode<'a>)>,
    generic_structs: HashMap<
        dream_types::DefId,
        &'a dream_syntax::nodes::struct_node::StructDeclarationNode<'a>,
    >,
    /// Every concrete `(base name, type args)` a generic class has been instantiated with (recorded
    /// by `ensure_struct_instantiated`). `node.structs` (the parsed AST) only ever holds the
    /// generic *template* declaration, never its monomorphizations — so `hir_build_imports`/
    /// `hir_build_intrinsics` (which need to emit a per-instantiation `(import ...)` or intrinsic
    /// binding for each of a generic class's `extern`/`@intrinsic` methods, mangled per instance)
    /// consult this list instead of `node.structs` to find every instantiation that needs one.
    generic_struct_instances: indexmap::IndexSet<dream_types::TypeId>,
    /// `@c` externs (by registered name) whose listed `fun` parameters need a per-target C wrapper,
    /// so a call site must pass a named function or a lambda literal there.
    c_wrapped_fun_params: HashMap<dream_types::DefId, Vec<usize>>,
    /// `@intrinsic` methods recorded at registration (`{Type}_{method}` DefId + key), including
    /// each generic monomorphization. [`hir_build_intrinsics`] merges this with free-function
    /// scan so codegen can dispatch by DefId.
    intrinsic_defs: Vec<(dream_types::DefId, String)>,
    struct_methods: Vec<(&'a FunctionNode<'a>, GenericBindings)>,
    /// Registered enums: name -> (member -> value). Enum values are plain `i32`s at runtime.
    enum_table: EnumTable,
    /// Layout of every registered (monomorphized) discriminated union.
    union_table: UnionTable,
    /// Generic discriminated-union templates (`enum Option<T> { ... }`), instantiated on demand.
    generic_unions: HashMap<dream_types::DefId, &'a EnumDeclarationNode<'a>>,
    /// Generic `extend Type<...> { ... }` templates (e.g. `extend Option<T> { ... }`), keyed by
    /// the extended type's name. Their methods are monomorphized alongside each concrete
    /// instantiation of the target generic union or struct (see `ensure_*_instantiated`).
    generic_extends: IndexMap<GenericExtendTarget, Vec<&'a ExtendNode<'a>>>,
    /// Interface name -> its method signatures in declaration order (the order is the interface's
    /// local method index, used for itable slot assignment). Each entry is a body-less
    /// [`FunctionNode`] (no implicit `this`). For interfaces that extend parents, this list is the
    /// flattened closure (parent methods, then own methods; child overrides replace parents).
    interface_methods: IndexMap<dream_types::TypeId, Vec<&'a FunctionNode<'a>>>,
    /// Generic interface templates (`interface Container<T> { ... }`), instantiated on demand into
    /// concrete `interface_methods` entries (e.g. `Container_int`) — mirrors `generic_structs`.
    generic_interfaces:
        HashMap<dream_types::DefId, &'a dream_syntax::nodes::InterfaceDeclarationNode<'a>>,
    /// Interface base name -> parent interface types from `: Parent (+ Parent)*` (unsubstituted
    /// when the interface is generic — substituted when building a concrete instance).
    interface_parents: HashMap<dream_types::DefId, Vec<Type>>,
    /// All interface declarations by base name (generic templates and concrete interfaces), used
    /// when flattening inheritance and looking up parent method defaults.
    interface_decls:
        HashMap<dream_types::DefId, &'a dream_syntax::nodes::InterfaceDeclarationNode<'a>>,
    /// Concrete interface name (mangled) -> immediate parent concrete interface names, recorded
    /// when the child's method list is flattened. Used to expand `implements` transitively.
    interface_parent_instances: HashMap<dream_types::TypeId, Vec<dream_types::TypeId>>,
    /// Mangled interface instances that already received `extend Iface<T>` package methods. Parent
    /// flattening can create `Collection_int` before `ensure_interface_instantiated("Collection")`
    /// runs; without this set the early-return would skip attaching `to_list`/`filter`/….
    interface_extensions_attached: HashSet<dream_types::TypeId>,
    /// Concrete array types (`int[]`, `Point[]`, …) that have already been monomorphized from the
    /// generic `extend T[] : IndexedCollection<T>` template.
    array_collections_attached: HashSet<dream_types::TypeId>,
    /// Class name -> the interfaces it implements (in `class C : A, B` order), recorded after the
    /// implements clause is validated. Names are mangled for generic instances (e.g. `Box_int` ->
    /// `Container_int`). Drives interface-typed assignability and itable emission. Includes
    /// transitive parent interfaces of each explicitly implemented interface.
    implements: HashMap<dream_types::TypeId, Vec<dream_types::TypeId>>,
    /// Type name (mangled for generic instances, matching `implements`'s keys) -> its
    /// `@operator`/`@cast`-tagged methods, populated by
    /// [`declarations::operator_overloads::Analyzer::validate_and_register_operator`] and consulted
    /// by `expressions::operators`/`expressions::dispatch`/`expressions::casts` to dispatch
    /// operators and user-defined conversions to the right method.
    operator_overloads:
        HashMap<dream_types::TypeId, declarations::operator_overloads::OperatorOverloads>,
    /// Type name (mangled for generic instances) -> `@get_indexer`/`@set_indexer`/`@iterator`/`@next` hooks,
    /// populated by [`declarations::protocol_hooks`] and consulted by indexer/`for..in` desugar.
    protocol_hooks: HashMap<dream_types::TypeId, declarations::protocol_hooks::ProtocolHooks>,
    /// Names of types declared `sealed` (class/struct/enum). A user `extend` block may not target
    /// any of these; compiler-synthesized extends (interface defaults) are exempt.
    sealed_types: HashSet<dream_types::DefId>,
    /// File/module-level visibility for enums and interfaces (types not tracked in the struct
    /// table): type name -> (declaring file, visibility). A non-public entry is only referenceable
    /// per [`Analyzer::visible_across_files`]. Absent or `None` file means always visible.
    type_visibility:
        HashMap<dream_types::DefId, (Option<Rc<str>>, dream_syntax::nodes::Visibility)>,
    /// Sink RC params moved into a field/index store; further uses of the binding are errors.
    moved_locals: HashSet<String>,
    /// An optional expected type for the expression currently being analyzed (from a `let`
    /// annotation or `return` type). Used to resolve the type arguments of a generic union's
    /// nullary variant (`let o: Option<int> = Option.None;`), where they cannot be inferred from
    /// arguments. `None` outside such contexts.
    current_expected_type: Option<Type>,
    /// The generic substitution bindings active while analyzing a monomorphized function or
    /// struct-method body. Empty outside of any generic instantiation. Used to resolve generic
    /// type parameters that appear inside a body (e.g. the `T` in `array_new<T>(...)`).
    current_generic_bindings: GenericBindings,
    /// The callee name (function/constructor) whose arguments are currently being analyzed, or
    /// `None` outside of any call. Consulted by `analyze_lambda` to apply `Task`-specific
    /// capture restrictions (see `is_task_body_call`) without threading a dedicated parameter
    /// through every call-argument-analysis helper.
    current_call_target_name: Option<String>,
    /// Stack of loop labels currently in scope, so `break label;`/`continue label;` can be
    /// validated against an enclosing labeled loop.
    loop_labels: Vec<String>,
    /// Label attached to the immediately-following loop (`outer: for ...`), consumed by that loop's
    /// analyzer so it can be threaded into the loop's HIR node. `None` for unlabeled loops.
    pending_loop_label: Option<String>,
    /// True while analyzing the body of an `async fun`. Gates the use of `await`.
    current_function_is_async: bool,
    /// True while analyzing the body of an `@unsafe fun`/method. Gates calling another `@unsafe`
    /// function/method — see `Analyzer::check_unsafe_call`.
    current_function_is_unsafe: bool,
    /// Runtime availability of the function whose body is currently being analyzed. Gates
    /// `check_runtime_call` on nested calls — see `Analyzer::check_runtime_call`.
    current_function_runtime: RuntimeSupport,
    /// Active compile-time runtime target(s) from the driver/CLI. Defaults to native-only.
    compile_targets: CompileTargets,
    /// Pointer facts used by the single HIR layout table.
    target_layout: dream_hir::TargetLayout,
    deferred_case_labels: Vec<Vec<(dream_hir::HExpr, Option<TextSpan>)>>,
    /// Overflow behavior stamped on integer arithmetic, set lexically by `checked`/`unchecked`
    /// blocks and reset at each function body.
    overflow: dream_hir::Overflow,
    /// The source file of the function whose body is currently being analyzed, used for
    /// file/module-level visibility checks at sites that do not thread `parent_function` (e.g.
    /// bare-identifier global reads). `None` outside any function body.
    current_file: Option<Rc<str>>,
    /// Maps each source file that declared a `module a.b.c;` to its dot-joined module path.
    /// Files absent from this map (the overwhelming majority: anyone who never writes `module`)
    /// belong to the implicit, unnamed root module. Derived from the module graph.
    file_modules: HashMap<Rc<str>, Rc<str>>,
    generated_files: HashSet<Rc<str>>,
    /// Every aliased `import a.b.c as x;` collected across all files (module path, item name,
    /// alias token, importing file path), populated once via [`Self::with_aliased_imports`] before
    /// [`Self::analyze`] runs. Drained by `register_import_aliases` (see `declarations::imports`)
    /// right after function registration, so aliases resolve against the fully-registered function
    /// table but are still available to every function body analyzed afterward.
    aliased_imports: Vec<(String, String, SyntaxToken, String)>,
    /// Resolved top-level variables, in declaration order. Surfaced to codegen via [`SemanticInfo`].
    globals: Vec<GlobalSymbol>,
    /// The module-level symbol scope holding every top-level variable. It is the root parent of
    /// every function's parameter table, so function bodies resolve global identifiers (and their
    /// `const`-ness) through ordinary lexical lookup.
    global_symbol_table: Rc<RefCell<SymbolTable>>,
    /// The structured type context (interner + def table). Nominal declarations register their
    /// `DefId` here and AST type annotations lower to interned `TypeId`s, so type identity,
    /// compatibility, and monomorphization keys move off strings onto the structured type system.
    type_ctx: TypeCtx,
    /// IDE side table: every name/member/call resolution recorded during body analysis (see
    /// [`ide`]). Purely additive — nothing in analysis reads it, so compiler output is unaffected.
    ide_refs: Vec<ide::IdeRef>,
    ide_sources: HashMap<dream_types::DefId, ide::IdeSource>,
    ide_member_sources: HashMap<(dream_types::DefId, String), ide::IdeSource>,
    /// Interleaved HIR-emission state and the accumulated emitted functions.
    hir: hir_emit::HirEmit,
    /// Library units have no process entry point.
    crate_type: CrateType,
}

/// Whether the compilation unit is a library or a binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrateType {
    #[default]
    Bin,
    Lib,
}

mod access;
mod contexts;
mod pipeline;
mod type_queries;
impl<'a> Analyzer<'a> {
    pub fn new(graph: &'a ModuleGraph<'a>, arena: &'a Bump) -> Self {
        let program = arena.alloc(graph.view());
        let mut type_ctx = TypeCtx::new();
        graph.configure_types(&mut type_ctx);
        Self {
            graph,
            program,
            function_table: FunctionTable::new(),
            struct_table: StructTable::new(),
            arena,
            generic_functions: HashMap::new(),
            instantiated_generics: IndexMap::new(),
            pending_lambdas: IndexMap::new(),
            lambda_counter: 0,
            boxed_locals: HashSet::new(),
            ref_boxed_locals: HashSet::new(),
            moved_locals: HashSet::new(),
            closure_captures: HashMap::new(),
            receiver_modes: HashMap::new(),
            capturing_fun_locals: HashMap::new(),
            is_binding_aliases: Vec::new(),
            generic_structs: HashMap::new(),
            generic_struct_instances: indexmap::IndexSet::new(),
            c_wrapped_fun_params: HashMap::new(),
            intrinsic_defs: Vec::new(),
            struct_methods: Vec::new(),
            enum_table: IndexMap::new(),
            union_table: IndexMap::new(),
            generic_unions: HashMap::new(),
            generic_extends: HashMap::new(),
            interface_methods: IndexMap::new(),
            generic_interfaces: HashMap::new(),
            interface_parents: HashMap::new(),
            interface_decls: HashMap::new(),
            interface_parent_instances: HashMap::new(),
            interface_extensions_attached: HashSet::new(),
            array_collections_attached: HashSet::new(),
            sealed_types: HashSet::new(),
            type_visibility: HashMap::new(),
            implements: HashMap::new(),
            operator_overloads: HashMap::new(),
            protocol_hooks: HashMap::new(),
            current_expected_type: None,
            current_generic_bindings: GenericBindings::new(),
            current_call_target_name: None,
            loop_labels: Vec::new(),
            pending_loop_label: None,
            current_function_is_async: false,
            current_function_is_unsafe: false,
            current_function_runtime: RuntimeSupport::ALL,
            compile_targets: CompileTargets::native_only(),
            target_layout: dream_hir::TargetLayout::default(),
            deferred_case_labels: Vec::new(),
            overflow: dream_hir::Overflow::Wrapping,
            current_file: None,
            file_modules: graph
                .files
                .iter()
                .filter_map(|file| {
                    let path = &graph.modules[file.module.0 as usize].path;
                    (!path.is_empty())
                        .then(|| (Rc::from(file.path.as_str()), Rc::from(path.as_str())))
                })
                .collect(),
            generated_files: graph
                .generated_files
                .iter()
                .map(|path| Rc::from(path.as_str()))
                .collect(),
            aliased_imports: Vec::new(),
            globals: Vec::new(),
            global_symbol_table: Rc::new(RefCell::new(SymbolTable::new(None))),
            type_ctx,
            ide_refs: Vec::new(),
            ide_sources: HashMap::new(),
            ide_member_sources: HashMap::new(),
            hir: hir_emit::HirEmit::default(),
            crate_type: CrateType::Bin,
        }
    }

    /// The type interner backing analysis. Its `TypeId`s are the ones referenced by the emitted HIR
    /// (`SemanticInfo::hir`), so the MIR backend must be handed *this* interner to lower that HIR.
    pub fn interner(&self) -> &dream_types::TypeInterner {
        &self.type_ctx.interner
    }

    /// Enables debug-info instrumentation so HIR emission interleaves [`dream_hir::HStmt::DebugLine`]
    /// source-line markers. Call before [`Self::analyze`].
    pub fn set_debug_info(&mut self, on: bool) {
        self.hir_set_debug_info(on);
    }

    /// Records every aliased `import a.b.c as x;` collected across all files, resolved once
    /// function registration completes (see `declarations::imports::register_import_aliases`).
    /// Call before [`Self::analyze`].
    pub fn with_aliased_imports(
        mut self,
        aliased_imports: Vec<(String, String, SyntaxToken, String)>,
    ) -> Self {
        self.aliased_imports = aliased_imports;
        self
    }

    /// Library vs binary compilation unit. Call before [`Self::analyze`].
    pub fn with_crate_type(mut self, crate_type: CrateType) -> Self {
        self.crate_type = crate_type;
        self
    }

    /// Active compile-time runtime target(s). Call before [`Self::analyze`].
    pub fn with_compile_targets(mut self, targets: CompileTargets) -> Self {
        self.compile_targets = targets;
        self
    }

    pub fn with_target_layout(mut self, target: dream_hir::TargetLayout) -> Self {
        self.target_layout = target;
        self
    }
    pub fn analyze(
        &mut self,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<SemanticInfo<'_>, SemanticError> {
        let pgm = self.program;
        self.analyze_pgm(pgm, diagnostics)
    }
}

#[cfg(test)]
#[path = "../tests/mod.rs"]
mod tests;
