use super::Analyzer;
use crate::gen_facts::{GenMethodView, GenTypeView};
use dream_abi::names::THIS_NAME;
use dream_syntax::nodes::Type;
use dream_types::TyKind;

impl Analyzer<'_> {
    /// Exports instantiated signatures and inherited members in interned type order.
    pub(super) fn generator_symbol_views(&self) -> Vec<GenTypeView> {
        let mut views = Vec::new();
        for (ty, kind) in self.type_ctx.interner.iter_kinds() {
            let def = match kind {
                TyKind::Struct(d, _)
                | TyKind::Union(d, _)
                | TyKind::Interface(d, _)
                | TyKind::Enum(d) => self.type_ctx.defs.get(*d),
                _ => continue,
            };
            let mut methods = Vec::new();
            for ((owner, name), candidates) in &self.function_table.methods {
                if *owner != ty {
                    continue;
                }
                for identity in candidates {
                    let Some(method) = self.function_table.functions.get(identity) else {
                        continue;
                    };
                    methods.push(GenMethodView {
                        name: name.clone(),
                        is_static: method.is_static,
                        is_async: method.is_async,
                        ret: self.type_ctx.syntax_type(method.resolved_return),
                        params: method
                            .parameters
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| {
                                method.param_names.get(*i).is_none_or(|n| n != THIS_NAME)
                            })
                            .map(|(i, p)| {
                                (
                                    method
                                        .param_names
                                        .get(i)
                                        .cloned()
                                        .unwrap_or_else(|| format!("arg{i}")),
                                    self.type_ctx.syntax_type(*p),
                                )
                            })
                            .collect(),
                    });
                }
            }
            if let Some(inherited) = self.interface_methods.get(&ty) {
                for method in inherited {
                    if methods.iter().any(|m| m.name == method.name.text) {
                        continue;
                    }
                    methods.push(GenMethodView {
                        name: method.name.text.clone(),
                        is_static: method.is_static,
                        is_async: method.is_async,
                        ret: method.return_type.clone().unwrap_or(Type::Void),
                        params: method
                            .parameters
                            .iter()
                            .filter(|p| p.name.text != THIS_NAME)
                            .map(|p| (p.name.text.clone(), p.type_.clone()))
                            .collect(),
                    });
                }
            }
            let bases = self
                .implements
                .get(&ty)
                .or_else(|| self.interface_parent_instances.get(&ty))
                .into_iter()
                .flatten()
                .map(|t| self.type_ctx.syntax_type(*t))
                .collect();
            let fields = self
                .struct_table
                .get_struct(ty)
                .map(|s| {
                    s.fields
                        .iter()
                        .map(|(n, f)| (n.clone(), self.type_ctx.syntax_type(f.ty)))
                        .collect()
                })
                .unwrap_or_default();
            let variants = self
                .union_table
                .get(&ty)
                .map(|u| {
                    u.variants
                        .iter()
                        .map(|v| {
                            (
                                v.name.clone(),
                                v.fields
                                    .iter()
                                    .map(|f| (f.name.clone(), self.type_ctx.syntax_type(f.ty)))
                                    .collect(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            views.push(GenTypeView {
                id: dream_abi::attributes::decl_identity(&def.module_path, &def.name),
                ty: self.type_ctx.syntax_type(ty),
                bases,
                fields,
                methods,
                variants,
            });
        }
        views
    }
}
