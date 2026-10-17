use super::model::*;
use super::snapshot::ProgramFacts;
use super::type_ref::type_ref;

/// Joins semantic instances to signature views while preserving source identities.
pub(super) fn attach(symbols: &mut [SymbolView], facts: &ProgramFacts<'_>) {
    for view in &facts.semantic.symbols {
        let Some(symbol) = symbols.iter_mut().find(|s| s.id == view.id) else {
            continue;
        };
        let declaration = symbol.declaration.as_ref();
        let field = |owner: &str, name: &str, ty: &dream_syntax::nodes::Type| {
            let original = declaration.and_then(|d| d.fields.iter().find(|f| f.name == name));
            let mut field = original.cloned().unwrap_or_else(|| Field {
                id: format!("{owner}.{name}"),
                name: name.to_string(),
                ..Default::default()
            });
            field.ty = type_ref(ty, facts.index, &[]);
            field
        };
        let methods = view
            .methods
            .iter()
            .map(|method| {
                let original = declaration.and_then(|d| {
                    d.methods
                        .iter()
                        .find(|m| m.name == method.name && m.params.len() == method.params.len())
                });
                let mut function = original.cloned().unwrap_or_else(|| Function {
                    id: format!("{}.{}", view.id, method.name),
                    name: method.name.clone(),
                    ..Default::default()
                });
                function.body = None;
                function.is_static = method.is_static;
                function.is_async = method.is_async;
                function.ret = type_ref(&method.ret, facts.index, &[]);
                let declared = function.params.clone();
                function.params = method
                    .params
                    .iter()
                    .map(|(name, ty)| {
                        let original = declared.iter().find(|p| &p.name == name);
                        Param {
                            name: name.clone(),
                            ty: type_ref(ty, facts.index, &[]),
                            is_ref: original.is_some_and(|p| p.is_ref),
                            attributes: original.map(|p| p.attributes.clone()).unwrap_or_default(),
                        }
                    })
                    .collect();
                function
            })
            .collect();
        let variants = view
            .variants
            .iter()
            .map(|(name, fields)| {
                let id = format!("{}.{name}", view.id);
                Variant {
                    id: id.clone(),
                    name: name.clone(),
                    fields: fields
                        .iter()
                        .map(|(name, ty)| field(&id, name, ty))
                        .collect(),
                    location: declaration.map(|d| d.location.clone()).unwrap_or_default(),
                }
            })
            .collect();
        symbol.instances.push(SymbolInstance {
            ty: type_ref(&view.ty, facts.index, &[]),
            bases: view
                .bases
                .iter()
                .map(|ty| type_ref(ty, facts.index, &[]))
                .collect(),
            fields: view
                .fields
                .iter()
                .map(|(name, ty)| field(&view.id, name, ty))
                .collect(),
            methods,
            variants,
        });
    }
}
