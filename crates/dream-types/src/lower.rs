//! [`TypeCtx`]: the analyzer-facing bundle of the [`TypeInterner`] and [`DefTable`], plus the
//! lowering from the AST [`Type`] to an interned [`TypeId`]. This is the bridge that lets the rest
//! of the compiler stop threading stringly-typed names around: declarations register their nominal
//! defs here, and every AST type annotation is lowered through [`TypeCtx::lower`].

use super::{DefId, DefKind, DefTable, ModuleId, PrimTy, TypeId, TypeInterner};
use dream_syntax::nodes::types::Type;
use indexmap::IndexMap;

/// Owns the interner and def table and remembers which nominal base names are structs/unions/enums
/// so AST `Type::Struct(name, args)` can be lowered to the right [`TyKind`](super::TyKind).
#[derive(Debug, Default)]
pub struct TypeCtx {
    pub interner: TypeInterner,
    pub defs: DefTable,
    /// Base name -> declared kind, used to disambiguate `Type::Struct` (which the parser also emits
    /// for unions and enums, since they are bare identifiers syntactically).
    declarations: IndexMap<(ModuleId, DefKind, String), DefId>,
    modules: IndexMap<ModuleId, String>,
    scope: ModuleId,
    imports: IndexMap<ModuleId, Vec<ModuleId>>,
    functions: IndexMap<(ModuleId, String, Vec<TypeId>), DefId>,
    methods: IndexMap<(TypeId, String, Vec<TypeId>), DefId>,
}

impl TypeCtx {
    pub fn new() -> Self {
        TypeCtx {
            interner: TypeInterner::new(),
            defs: DefTable::new(),
            declarations: IndexMap::new(),
            modules: IndexMap::new(),
            scope: ModuleId::ROOT,
            imports: IndexMap::new(),
            functions: IndexMap::new(),
            methods: IndexMap::new(),
        }
    }

    pub fn register(
        &mut self,
        kind: DefKind,
        name: &str,
        generic_params: Vec<String>,
    ) -> super::DefId {
        let key = (self.scope, kind, name.to_string());
        if let Some(&def) = self.declarations.get(&key) {
            return def;
        }
        let path = self
            .modules
            .get(&self.scope)
            .map(String::as_str)
            .unwrap_or("");
        let def = self
            .defs
            .allocate(self.scope, path, kind, name, generic_params);
        self.declarations.insert(key, def);
        match kind {
            DefKind::Struct => {
                self.interner.struct_ty(def, vec![]);
            }
            DefKind::Union => {
                self.interner.union_ty(def, vec![]);
            }
            DefKind::Enum => {
                self.interner.enum_ty(def);
            }
            DefKind::Interface => {
                self.interner.interface_ty(def, vec![]);
            }
            DefKind::Function => {}
        }
        def
    }

    pub fn nominal_kind(&self, name: &str) -> Option<DefKind> {
        [
            DefKind::Struct,
            DefKind::Union,
            DefKind::Enum,
            DefKind::Interface,
        ]
        .iter()
        .copied()
        .find(|kind| self.resolve(*kind, name).is_some())
    }

    pub fn define_module(&mut self, id: ModuleId, path: String, imports: Vec<ModuleId>) {
        self.modules.insert(id, path);
        self.imports.insert(id, imports);
    }

    pub fn set_scope(&mut self, scope: ModuleId) {
        self.scope = scope;
    }

    pub fn scope(&self) -> ModuleId {
        self.scope
    }

    pub fn module_path(&self, module: ModuleId) -> Option<&str> {
        self.modules.get(&module).map(String::as_str)
    }

    pub fn visible_modules(&self) -> Vec<ModuleId> {
        let mut modules = indexmap::IndexSet::new();
        modules.insert(self.scope);
        modules.insert(ModuleId::ROOT);
        modules.extend(self.imports.get(&self.scope).into_iter().flatten().copied());
        modules.into_iter().collect()
    }

    pub fn register_function(&mut self, name: &str, parameters: &[TypeId]) -> DefId {
        let key = (self.scope, name.to_string(), parameters.to_vec());
        if let Some(&def) = self.functions.get(&key) {
            return def;
        }
        let path = self.module_path(self.scope).unwrap_or("").to_string();
        let def = self
            .defs
            .allocate(self.scope, &path, DefKind::Function, name, vec![]);
        self.functions.insert(key, def);
        self.declarations
            .entry((self.scope, DefKind::Function, name.to_string()))
            .or_insert(def);
        def
    }

    pub fn register_method(&mut self, owner: TypeId, member: &str, parameters: &[TypeId]) -> DefId {
        let key = (owner, member.to_string(), parameters.to_vec());
        if let Some(&def) = self.methods.get(&key) {
            return def;
        }
        let name = super::method_fn(
            &super::type_symbol(&self.interner, &self.defs, owner),
            member,
        );
        let path = self.module_path(self.scope).unwrap_or("").to_string();
        let def = self
            .defs
            .allocate(self.scope, &path, DefKind::Function, &name, vec![]);
        self.methods.insert(key, def);
        def
    }

    pub fn resolve_function(&self, name: &str, parameters: &[TypeId]) -> Option<DefId> {
        let lookup = |module, name: &str| {
            self.functions
                .get(&(module, name.to_string(), parameters.to_vec()))
                .copied()
        };
        if let Some((path, local)) = name.rsplit_once("::") {
            let module = self
                .modules
                .iter()
                .find_map(|(&id, value)| (value == path).then_some(id))?;
            return lookup(module, name).or_else(|| lookup(module, local));
        }
        if let Some(def) = lookup(self.scope, name).or_else(|| lookup(ModuleId::ROOT, name)) {
            return Some(def);
        }
        let mut matches = self
            .imports
            .get(&self.scope)
            .into_iter()
            .flatten()
            .filter_map(|&module| lookup(module, name));
        let def = matches.next()?;
        matches.all(|other| other == def).then_some(def)
    }

    /// The definition `name` declares in exactly `module`, ignoring imports and the root fallback.
    pub fn declared_in(&self, module: ModuleId, kind: DefKind, name: &str) -> Option<DefId> {
        self.declarations
            .get(&(module, kind, name.to_string()))
            .copied()
    }

    pub fn resolve(&self, kind: DefKind, name: &str) -> Option<DefId> {
        if let Some((path, name)) = name.rsplit_once("::") {
            let module = self
                .modules
                .iter()
                .find_map(|(&id, module)| (module == path).then_some(id))?;
            return self
                .declarations
                .get(&(module, kind, format!("{path}::{name}")))
                .or_else(|| self.declarations.get(&(module, kind, name.to_string())))
                .copied();
        }
        self.resolve_from(self.scope, kind, name)
    }

    /// Resolves an unqualified source name as written inside `scope`: the module itself, then the
    /// root module, then an unambiguous import.
    pub fn resolve_from(&self, scope: ModuleId, kind: DefKind, name: &str) -> Option<DefId> {
        let lookup = |module| {
            self.declarations
                .get(&(module, kind, name.to_string()))
                .copied()
        };
        if let Some(def) = lookup(scope).or_else(|| lookup(ModuleId::ROOT)) {
            return Some(def);
        }
        let mut matches = self
            .imports
            .get(&scope)
            .into_iter()
            .flatten()
            .filter_map(|&module| lookup(module));
        let def = matches.next()?;
        matches.all(|other| other == def).then_some(def)
    }

    /// True when the unqualified `name` denotes `def` from every module's lexical scope, so a
    /// syntax type can spell it bare without changing meaning wherever it is lowered.
    pub fn resolves_everywhere(&self, def: DefId, name: &str) -> bool {
        let kind = self.defs.get(def).kind;
        std::iter::once(ModuleId::ROOT)
            .chain(self.modules.keys().copied())
            .all(|module| self.resolve_from(module, kind, name) == Some(def))
    }

    /// Resolves a lexical source name; composite types must already have a structural shape.
    pub fn resolved_type(&self, name: &str) -> Option<TypeId> {
        use super::TyKind;
        if let Some(primitive) = PrimTy::from_name(name) {
            return self.interner.lookup(&TyKind::Prim(primitive));
        }
        match name {
            "object" => return Some(self.interner.object()),
            "void" => return Some(self.interner.void()),
            "js" => return Some(self.interner.js()),
            _ => {}
        }
        let kind = self.nominal_kind(name)?;
        let def = self.resolve(kind, name)?;
        self.interner.lookup(&match kind {
            DefKind::Struct => TyKind::Struct(def, vec![]),
            DefKind::Union => TyKind::Union(def, vec![]),
            DefKind::Enum => TyKind::Enum(def),
            DefKind::Interface => TyKind::Interface(def, vec![]),
            DefKind::Function => return None,
        })
    }

    pub fn instantiate(&mut self, def: DefId, args: Vec<TypeId>) -> TypeId {
        match self.defs.get(def).kind {
            DefKind::Struct => self.interner.struct_ty(def, args),
            DefKind::Union => self.interner.union_ty(def, args),
            DefKind::Interface => self.interner.interface_ty(def, args),
            DefKind::Enum => self.interner.enum_ty(def),
            DefKind::Function => self.interner.error(),
        }
    }

    /// Lowers an AST type to an interned id with no generic substitution in scope.
    pub fn lower(&mut self, ty: &Type) -> TypeId {
        self.lower_with(ty, &IndexMap::new())
    }

    /// Lowers an AST type, substituting any in-scope generic parameter names (`T`) with the bound
    /// concrete id. Unbound generic names lower to the poison `Error` type.
    pub fn lower_with(&mut self, ty: &Type, bindings: &IndexMap<String, TypeId>) -> TypeId {
        match ty {
            Type::Integer(_) => self.interner.prim(PrimTy::Int),
            Type::UInt(_) => self.interner.prim(PrimTy::UInt),
            Type::Long(_) => self.interner.prim(PrimTy::Long),
            Type::ULong(_) => self.interner.prim(PrimTy::ULong),
            Type::ISize(_) => self.interner.prim(PrimTy::ISize),
            Type::USize(_) => self.interner.prim(PrimTy::USize),
            Type::Byte(_) => self.interner.prim(PrimTy::Byte),
            Type::Float(_) => self.interner.prim(PrimTy::Float),
            Type::Double(_) => self.interner.prim(PrimTy::Double),
            Type::Boolean(_) => self.interner.prim(PrimTy::Bool),
            Type::Char(_) => self.interner.prim(PrimTy::Char),
            Type::String(_) => self.interner.prim(PrimTy::String),
            Type::Object(_) => self.interner.object(),
            Type::Void => self.interner.void(),
            Type::Unknown => self.interner.error(),
            Type::GenericFunctionItem(_) => self.interner.error(),
            Type::Array(inner) => {
                let e = self.lower_with(inner, bindings);
                self.interner.array(e)
            }
            Type::Tuple(elems) => {
                let ids = elems.iter().map(|e| self.lower_with(e, bindings)).collect();
                self.interner.tuple_ty(ids)
            }
            Type::Function(params, ret) => {
                let ps = params
                    .iter()
                    .map(|p| self.lower_with(p, bindings))
                    .collect();
                let r = self.lower_with(ret, bindings);
                self.interner.func(ps, r)
            }
            Type::Generic(name) => bindings
                .get(name)
                .copied()
                .unwrap_or_else(|| self.interner.error()),
            Type::Struct(token, generic_args) => {
                let name = &token.text;
                if let Some(bound) = bindings.get(name) {
                    return *bound;
                }
                if generic_args.is_none()
                    && let Some(id) = self.resolved_type(name)
                {
                    return id;
                }
                let args: Vec<TypeId> = generic_args
                    .as_ref()
                    .map(|gs| gs.iter().map(|g| self.lower_with(g, bindings)).collect())
                    .unwrap_or_default();
                match self.nominal_kind(name) {
                    Some(DefKind::Enum) => {
                        let Some(def) = self.resolve(DefKind::Enum, name) else {
                            return self.interner.error();
                        };
                        self.interner.enum_ty(def)
                    }
                    Some(DefKind::Union) => {
                        let Some(def) = self.resolve(DefKind::Union, name) else {
                            return self.interner.error();
                        };
                        self.interner.union_ty(def, args)
                    }
                    Some(DefKind::Interface) => {
                        let Some(def) = self.resolve(DefKind::Interface, name) else {
                            return self.interner.error();
                        };
                        self.interner.interface_ty(def, args)
                    }
                    _ => {
                        let Some(def) = self.resolve(DefKind::Struct, name) else {
                            return self.interner.error();
                        };
                        self.interner.struct_ty(def, args)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TyKind, display_name};
    use dream_syntax::token::syntax_token::SyntaxToken;
    use dream_syntax::token::token_kind::TokenKind;
    use dream_text::line_text::LineText;
    use dream_text::text_span::TextSpan;

    fn ident(text: &str) -> SyntaxToken {
        let lt = LineText::new(String::new());
        let span = TextSpan::new((0, 0), &lt);
        SyntaxToken::new(TokenKind::IdentifierToken, span, text.to_string())
    }

    #[test]
    fn lowers_primitive_and_array() {
        let mut ctx = TypeCtx::new();
        let arr = Type::Array(Box::new(Type::Integer(ident("int"))));
        let id = ctx.lower(&arr);
        assert!(matches!(ctx.interner.kind(id), TyKind::Array(_)));
        assert_eq!(display_name(&ctx.interner, &ctx.defs, id), "int[]");
    }

    #[test]
    fn lowers_registered_struct_with_args() {
        let mut ctx = TypeCtx::new();
        ctx.register(DefKind::Struct, "Box", vec!["T".to_string()]);
        let ty = Type::Struct(ident("Box"), Some(vec![Type::Integer(ident("int"))]));
        let id = ctx.lower(&ty);
        assert_eq!(display_name(&ctx.interner, &ctx.defs, id), "Box<int>");
    }

    #[test]
    fn generic_binding_substitutes() {
        let mut ctx = TypeCtx::new();
        let mut bindings = IndexMap::new();
        let int = ctx.interner.int();
        bindings.insert("T".to_string(), int);
        let ty = Type::Generic("T".to_string());
        assert_eq!(ctx.lower_with(&ty, &bindings), int);
    }

    #[test]
    fn nested_arguments_do_not_collide_with_source_identifiers() {
        let mut ctx = TypeCtx::new();
        let boxed = ctx.register(DefKind::Struct, "Box", vec!["T".to_string()]);
        let a_b = ctx.register(DefKind::Struct, "A_B", vec![]);
        let a = ctx.register(DefKind::Struct, "A", vec!["T".to_string()]);
        let b = ctx.register(DefKind::Struct, "B", vec![]);
        let a_b = ctx.instantiate(a_b, vec![]);
        let b = ctx.instantiate(b, vec![]);
        let nested = ctx.instantiate(a, vec![b]);
        let first = ctx.instantiate(boxed, vec![a_b]);
        let second = ctx.instantiate(boxed, vec![nested]);
        assert_ne!(first, second);
        assert_eq!(ctx.lower(&ctx.syntax_type(first)), first);
        assert_eq!(ctx.lower(&ctx.syntax_type(second)), second);
        assert!(ctx.resolved_type("Box_A_B").is_none());
        assert_eq!(
            ctx.lower(&Type::Struct(ident("int[]"), None)),
            ctx.interner.error()
        );
    }

    #[test]
    fn reconstructed_foreign_arguments_preserve_definition_identity() {
        let mut ctx = TypeCtx::new();
        let caller = ModuleId(1);
        let library = ModuleId(2);
        ctx.define_module(caller, "caller".to_string(), vec![library]);
        ctx.define_module(library, "library".to_string(), vec![]);
        ctx.set_scope(caller);
        let local = ctx.register(DefKind::Struct, "Value", vec![]);
        let local = ctx.instantiate(local, vec![]);
        let array = ctx.interner.array(local);
        ctx.set_scope(library);
        let foreign = ctx.register(DefKind::Struct, "Value", vec![]);
        let foreign = ctx.instantiate(foreign, vec![]);
        let container = ctx.register(DefKind::Struct, "Container", vec!["T".to_string()]);
        let instance = ctx.instantiate(container, vec![array]);
        let syntax = ctx.syntax_type(instance);
        assert_eq!(ctx.lower(&syntax), instance);
        assert_ne!(local, foreign);
        ctx.set_scope(caller);
        assert_eq!(ctx.lower(&syntax), instance);
    }

    #[test]
    fn method_identity_includes_the_concrete_receiver() {
        let mut ctx = TypeCtx::new();
        let def = ctx.register(DefKind::Struct, "Box", vec!["T".to_string()]);
        let integer = ctx.instantiate(def, vec![ctx.interner.int()]);
        let string = ctx.instantiate(def, vec![ctx.interner.string()]);
        let first = ctx.register_method(integer, "read", &[]);
        let second = ctx.register_method(string, "read", &[]);
        assert_ne!(first, second);
        assert_eq!(ctx.register_method(integer, "read", &[]), first);
    }
}
