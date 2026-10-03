//! [`TypeCtx`]: the analyzer-facing bundle of the [`TypeInterner`] and [`DefTable`], plus the
//! lowering from the AST [`Type`] to an interned [`TypeId`]. This is the bridge that lets the rest
//! of the compiler stop threading stringly-typed names around: declarations register their nominal
//! defs here, and every AST type annotation is lowered through [`TypeCtx::lower`].

use super::{DefId, DefKind, DefTable, ModuleId, PrimTy, TypeId, TypeInterner};
use dream_syntax::nodes::types::{mangle_generic, Type};
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
    /// Mangled monomorphization name (`List_JsonValue`) -> the canonical interned id of that generic
    /// instance (`Struct(List_def, [JsonValue])`). The analyzer registers each instantiation here so
    /// the pre-mangled bare spelling and the structured `List<JsonValue>` spelling lower to the same
    /// [`TypeId`].
    instances: IndexMap<(ModuleId, String), TypeId>,
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
            instances: IndexMap::new(),
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

    pub fn instance_name(&self, ty: TypeId) -> String {
        match self.interner.kind(ty) {
            super::TyKind::Struct(def, args)
            | super::TyKind::Union(def, args)
            | super::TyKind::Interface(def, args) => self
                .instances
                .iter()
                .find_map(|((module, name), &id)| {
                    (id == ty && *module == def.module).then(|| name.clone())
                })
                .unwrap_or_else(|| {
                    if args.is_empty() {
                        self.defs.name(*def).to_string()
                    } else {
                        super::display_name(&self.interner, &self.defs, ty)
                    }
                }),
            _ => super::display_name(&self.interner, &self.defs, ty),
        }
    }

    pub fn resolve(&self, kind: DefKind, name: &str) -> Option<DefId> {
        if let Some(ty) = self.instance_alias(name) {
            match (kind, self.interner.kind(ty)) {
                (DefKind::Struct, super::TyKind::Struct(def, _))
                | (DefKind::Union, super::TyKind::Union(def, _))
                | (DefKind::Interface, super::TyKind::Interface(def, _)) => return Some(*def),
                _ => {}
            }
        }
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
        let lookup = |module| {
            self.declarations
                .get(&(module, kind, name.to_string()))
                .copied()
        };
        if let Some(def) = lookup(self.scope).or_else(|| lookup(ModuleId::ROOT)) {
            return Some(def);
        }
        let mut matches = self
            .imports
            .get(&self.scope)
            .into_iter()
            .flatten()
            .filter_map(|&module| lookup(module));
        let def = matches.next()?;
        matches.all(|other| other == def).then_some(def)
    }

    pub fn resolved_type(&self, name: &str) -> Option<TypeId> {
        use super::TyKind;
        if let Some(primitive) = PrimTy::from_name(name) {
            return self.interner.lookup(&TyKind::Prim(primitive));
        }
        if let Some(base) = name.strip_suffix("[]") {
            return self
                .interner
                .lookup(&TyKind::Array(self.resolved_type(base)?));
        }
        if let Some(id) = self.instance_alias(name) {
            return Some(id);
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

    fn instance_alias(&self, name: &str) -> Option<TypeId> {
        let lookup = |module| self.instances.get(&(module, name.to_string())).copied();
        lookup(self.scope)
            .or_else(|| lookup(ModuleId::ROOT))
            .or_else(|| {
                let mut found = self
                    .imports
                    .get(&self.scope)
                    .into_iter()
                    .flatten()
                    .filter_map(|&m| lookup(m));
                let id = found.next()?;
                found.all(|other| other == id).then_some(id)
            })
    }

    /// Records a generic instantiation so its mangled bare name canonicalizes to the structured
    /// `(base def, args)` id. `kind` is the base's kind (`Struct`/`Union`), `base` its source name,
    /// and `args` the concrete type arguments. Returns the canonical id. Idempotent.
    ///
    /// The mangled name is identity-defining, so the first registration wins: a later call whose
    /// `base` is itself the already-mangled name with no args (e.g. a field access on a value typed
    /// `Box_string`, which lowers `("Box_string", [])` rather than `("Box", [string])`) must not
    /// clobber the canonical `(base def, args)` id with a bogus nominal `struct_ty(Box_string, [])`.
    pub fn register_instance(&mut self, kind: DefKind, base: &str, args: &[Type]) -> TypeId {
        let mangled = mangle_generic(base, args);
        if let Some(id) = self.instance_alias(&mangled) {
            return id;
        }
        let arg_ids: Vec<TypeId> = args.iter().map(|a| self.lower(a)).collect();
        let Some(def) = self.resolve(kind, base) else {
            return self.interner.error();
        };
        let id = match kind {
            DefKind::Union => self.interner.union_ty(def, arg_ids),
            DefKind::Interface => self.interner.interface_ty(def, arg_ids),
            _ => self.interner.struct_ty(def, arg_ids),
        };
        self.instances.insert((def.module, mangled), id);
        id
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
                // A bare name (no structured args) may be stringly-reconstructed and encode an array
                // suffix, a primitive spelling, or a pre-mangled generic instance; route it through
                // name-based lowering so every spelling of a type interns identically.
                if generic_args.is_none() {
                    return self.lower_name(name, bindings);
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

    /// Lowers a bare type *name* (as opposed to a structured AST node) to an interned id, absorbing
    /// string spellings still used by signatures/tables: array (`T[]`) suffixes, primitive names,
    /// `object`/`void`, pre-mangled generic instances, and nominal references. This keeps every
    /// spelling of the same type interning to one [`TypeId`].
    fn lower_name(&mut self, name: &str, bindings: &IndexMap<String, TypeId>) -> TypeId {
        if let Some(&bound) = bindings.get(name) {
            return bound;
        }
        if let Some(base) = name.strip_suffix("[]") {
            let inner = self.lower_name(base, bindings);
            return self.interner.array(inner);
        }
        if let Some(prim) = PrimTy::from_name(name) {
            return self.interner.prim(prim);
        }
        match name {
            "object" => return self.interner.object(),
            "void" => return self.interner.void(),
            // The dynamic JS-interop type is a distinguished non-reference i32 handle, not a nominal
            // struct, so recognize its bare name here before the nominal fallback.
            "js" => return self.interner.js(),
            _ => {}
        }
        if let Some(id) = self.instance_alias(name) {
            return id;
        }
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
                self.interner.union_ty(def, vec![])
            }
            Some(DefKind::Interface) => {
                let Some(def) = self.resolve(DefKind::Interface, name) else {
                    return self.interner.error();
                };
                self.interner.interface_ty(def, vec![])
            }
            Some(DefKind::Struct) => {
                let Some(def) = self.resolve(DefKind::Struct, name) else {
                    return self.interner.error();
                };
                self.interner.struct_ty(def, vec![])
            }
            // An unregistered name (or a function name used in type position) is not a known type.
            // Interning it as a nominal struct here is exactly the fragility hazard from the review:
            // a typo or interning-drift would silently mint a bogus type and miscompile. Lower it to
            // the poison `Error` type instead, which `compat.rs` already suppresses so the real
            // diagnostic (raised where the name was resolved) is what surfaces.
            Some(DefKind::Function) | None => self.interner.error(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{display_name, TyKind};
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
}
