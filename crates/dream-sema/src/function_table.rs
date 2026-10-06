use crate::errors::SymbolError;
use dream_types::{DefId, ModuleId, TyKind, TypeCtx, TypeId, TypeInterner};
use indexmap::IndexMap;

mod info;
pub use info::FunctionTableInfo;

pub type FunctionIdentity = (DefId, Vec<TypeId>);

#[derive(Debug, Clone, Default)]
pub struct FunctionTable {
    pub functions: IndexMap<FunctionIdentity, FunctionTableInfo>,
    // Names only index lexical source scopes; selected functions retain their typed identity.
    scopes: IndexMap<(ModuleId, String), Vec<FunctionIdentity>>,
    pub methods: IndexMap<(TypeId, String), Vec<FunctionIdentity>>,
    pub generic_methods: IndexMap<(TypeId, String), DefId>,
    declarations: IndexMap<usize, FunctionIdentity>,
}

pub enum OverloadResolution {
    Unique(FunctionIdentity),
    None,
    Ambiguous(Vec<FunctionIdentity>),
}

impl FunctionTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_declaration(
        &mut self,
        node: &dream_syntax::nodes::FunctionNode<'_>,
        identity: FunctionIdentity,
    ) {
        self.declarations
            .insert(node as *const _ as usize, identity);
    }

    pub fn declaration_node(
        &self,
        node: &dream_syntax::nodes::FunctionNode<'_>,
    ) -> Option<FunctionIdentity> {
        self.declarations.get(&(node as *const _ as usize)).cloned()
    }

    pub fn method_candidates(&self, owner: TypeId, member: &str) -> Vec<FunctionIdentity> {
        self.methods
            .get(&(owner, member.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    pub fn add_method(
        &mut self,
        owner: TypeId,
        member: &str,
        info: FunctionTableInfo,
    ) -> Result<FunctionIdentity, SymbolError> {
        let identity = info.identity.clone();
        let functions = &self.functions;
        let candidates = self.methods.entry((owner, member.to_string())).or_default();
        if candidates.iter().any(|key| {
            functions
                .get(key)
                .is_some_and(|old| old.parameters == info.parameters)
        }) {
            return Err(SymbolError::new(format!(
                "Duplicate overload: '{member}' with the same parameter types is already defined"
            )));
        }
        candidates.push(identity.clone());
        self.functions.insert(identity.clone(), info);
        Ok(identity)
    }

    pub fn add_overload(
        &mut self,
        base: &str,
        info: FunctionTableInfo,
        _type_ctx: &mut TypeCtx,
    ) -> Result<FunctionIdentity, SymbolError> {
        let identity = info.identity.clone();
        let scope = (identity.0.module, base.to_string());
        let functions = &self.functions;
        let candidates = self.scopes.entry(scope).or_default();
        if candidates.iter().any(|key| {
            functions
                .get(key)
                .is_some_and(|old| old.parameters == info.parameters)
        }) {
            return Err(SymbolError::new(format!(
                "Duplicate overload: '{base}' with the same parameter types is already defined"
            )));
        }
        candidates.push(identity.clone());
        self.functions.insert(identity.clone(), info);
        Ok(identity)
    }

    pub fn add_instance(
        &mut self,
        mut info: FunctionTableInfo,
        def: DefId,
        args: Vec<TypeId>,
    ) -> FunctionIdentity {
        let identity = (def, args);
        info.identity = identity.clone();
        self.functions.entry(identity.clone()).or_insert(info);
        identity
    }

    pub fn add_function(
        &mut self,
        name: String,
        info: FunctionTableInfo,
    ) -> Result<FunctionIdentity, SymbolError> {
        let identity = info.identity.clone();
        let scope = (identity.0.module, name.clone());
        if self.scopes.contains_key(&scope) {
            return Err(SymbolError::new(format!(
                "Function already exists ({name})"
            )));
        }
        self.scopes.insert(scope, vec![identity.clone()]);
        self.functions.insert(identity.clone(), info);
        Ok(identity)
    }

    pub fn bind_alias(
        &mut self,
        module: ModuleId,
        name: &str,
        identities: Vec<FunctionIdentity>,
    ) -> Result<(), SymbolError> {
        let key = (module, name.to_string());
        if self.scopes.contains_key(&key) {
            return Err(SymbolError::new(format!(
                "Function already exists ({name})"
            )));
        }
        self.scopes.insert(key, identities);
        Ok(())
    }

    pub fn candidates_in(&self, module: ModuleId, name: &str) -> &[FunctionIdentity] {
        self.scopes
            .get(&(module, name.to_string()))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn candidates(&self, ctx: &TypeCtx, name: &str) -> Vec<FunctionIdentity> {
        if let Some((path, item)) = name.rsplit_once("::") {
            return self
                .scopes
                .iter()
                .find_map(|((module, source), keys)| {
                    (source == item && ctx.module_path(*module) == Some(path)).then(|| keys.clone())
                })
                .unwrap_or_default();
        }
        let mut imported = Vec::new();
        for module in ctx.visible_modules() {
            let keys = self.candidates_in(module, name);
            if keys.is_empty() {
                continue;
            }
            if module == ctx.scope() || module == ModuleId::ROOT {
                return keys.to_vec();
            }
            imported.extend_from_slice(keys);
        }
        imported
    }

    pub fn lookup(&self, ctx: &TypeCtx, name: &str) -> Result<FunctionTableInfo, SymbolError> {
        let candidates = self.candidates(ctx, name);
        match candidates.as_slice() {
            [identity] => self.get_function(identity),
            [] => Err(SymbolError::new(format!(
                "Function does not exist ({name})"
            ))),
            _ => Err(SymbolError::new(format!(
                "Function does not resolve uniquely ({name})"
            ))),
        }
    }

    pub fn get_function(
        &self,
        identity: &FunctionIdentity,
    ) -> Result<FunctionTableInfo, SymbolError> {
        self.functions
            .get(identity)
            .cloned()
            .ok_or_else(|| SymbolError::new(format!("Function does not exist ({identity:?})")))
    }

    pub fn is_overloaded(&self, ctx: &TypeCtx, name: &str) -> bool {
        self.candidates(ctx, name).len() > 1
    }

    pub fn emitted_name(&self, ctx: &TypeCtx, identity: &FunctionIdentity) -> String {
        let Some(info) = self.functions.get(identity) else {
            return ctx.defs.name(identity.0).to_string();
        };
        let overloaded = self.candidates_in(identity.0.module, &info.name).len() > 1
            || self
                .methods
                .values()
                .any(|keys| keys.len() > 1 && keys.contains(identity));
        let method = self.methods.values().any(|keys| keys.contains(identity));
        let name = if method {
            ctx.defs.name(identity.0)
        } else {
            &info.name
        };
        if !overloaded {
            return name.to_string();
        }
        let parts: Vec<_> = info
            .parameters
            .iter()
            .map(|&ty| dream_types::type_symbol(&ctx.interner, &ctx.defs, ty))
            .collect();
        format!("{}.{}", name, parts.join("."))
    }

    pub fn declaration(
        &self,
        ctx: &TypeCtx,
        name: &str,
        parameters: &[TypeId],
    ) -> Option<FunctionIdentity> {
        self.candidates(ctx, name).into_iter().find(|key| {
            self.functions
                .get(key)
                .is_some_and(|info| info.parameters == parameters && key.1.is_empty())
        })
    }

    pub fn overload_with_params(
        &self,
        ctx: &TypeCtx,
        name: &str,
        parameters: &[TypeId],
    ) -> Option<FunctionIdentity> {
        self.candidates(ctx, name).into_iter().find(|key| {
            self.functions
                .get(key)
                .is_some_and(|info| !info.is_variadic && info.parameters == parameters)
        })
    }

    pub fn select_overload(
        &self,
        ctx: &TypeCtx,
        base: &str,
        args: &[TypeId],
        mut compatible: impl FnMut(TypeId, TypeId) -> bool,
    ) -> OverloadResolution {
        self.select_candidates(ctx, self.candidates(ctx, base), args, &mut compatible)
    }

    pub fn select_candidates(
        &self,
        ctx: &TypeCtx,
        candidates: Vec<FunctionIdentity>,
        args: &[TypeId],
        mut compatible: impl FnMut(TypeId, TypeId) -> bool,
    ) -> OverloadResolution {
        let scored: Vec<_> = candidates
            .into_iter()
            .filter_map(|key| {
                let info = self.functions.get(&key)?;
                Self::score(info, args, &ctx.interner, &mut compatible).map(|score| (score, key))
            })
            .collect();
        let Some(max) = scored.iter().map(|(score, _)| *score).max() else {
            return OverloadResolution::None;
        };
        let mut best: Vec<_> = scored
            .into_iter()
            .filter(|(score, _)| *score == max)
            .map(|(_, key)| key)
            .collect();
        if best.len() == 1 {
            OverloadResolution::Unique(best.remove(0))
        } else {
            OverloadResolution::Ambiguous(best)
        }
    }

    fn score(
        info: &FunctionTableInfo,
        args: &[TypeId],
        interner: &TypeInterner,
        compatible: &mut impl FnMut(TypeId, TypeId) -> bool,
    ) -> Option<i32> {
        let mut score_pairs = |parameters: &[TypeId], arguments: &[TypeId]| {
            parameters
                .iter()
                .zip(arguments)
                .try_fold(0, |score, (&param, &arg)| {
                    if param == arg {
                        Some(score + 1)
                    } else if compatible(param, arg) {
                        Some(score)
                    } else {
                        None
                    }
                })
        };
        if !info.is_variadic {
            if args.len() < info.required_params() || args.len() > info.parameters.len() {
                return None;
            }
            return score_pairs(&info.parameters, args)
                .map(|score| score + i32::from(args.len() == info.parameters.len()));
        }
        let fixed = info.parameters.len().checked_sub(1)?;
        let required = info
            .defaults
            .iter()
            .take(fixed)
            .position(Option::is_some)
            .unwrap_or(fixed);
        if args.len() < required {
            return None;
        }
        if args.len() == info.parameters.len()
            && let Some(score) = score_pairs(&info.parameters, args) {
                return Some(score + 1);
            }
        let TyKind::Array(element) = interner.kind(info.parameters[fixed]) else {
            return None;
        };
        let mut score = score_pairs(&info.parameters[..fixed], &args[..args.len().min(fixed)])?;
        for &arg in args.iter().skip(fixed) {
            score += score_pairs(std::slice::from_ref(element), &[arg])?;
        }
        Some(score + i32::from(args.len() == fixed))
    }
}
