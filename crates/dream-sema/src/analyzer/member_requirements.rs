//! `@requires_member` checks at generic call sites. A requirement names a method the bound type
//! must provide; a method found on a generic template carries its own requirements on that
//! template's parameters, which are checked recursively so `Map<string, object>` fails at the
//! caller instead of inside the library body.

use super::*;
use dream_abi::attributes::member_requirements;
use dream_syntax::nodes::FunctionNode;
use dream_types::{TyKind, TypeId};

/// Nesting allowed before a requirement chain is treated as unsatisfied.
const MAX_REQUIREMENT_DEPTH: usize = 32;

impl<'a> Analyzer<'a> {
    /// Reports the first unmet `@requires_member` on `template` under `bindings` at `position`.
    pub(in crate::analyzer) fn member_requirements_met(
        &mut self,
        template: &FunctionNode<'a>,
        bindings: &GenericBindings,
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        for (parameter, member, message) in member_requirements(&template.attributes) {
            let Some(bound) = bindings.get(&parameter) else {
                continue;
            };
            let ty = self.type_ctx.lower(bound);
            if self.type_provides_member(ty, &member, 0) {
                continue;
            }
            let shown = self.type_id_display(ty);
            let message = match message {
                Some(message) => message.replace("{type}", &shown),
                None => format!("type '{shown}' has no member '{member}'"),
            };
            diagnostics.report_error(message, Some(position));
            return false;
        }
        true
    }

    /// Whether a method on a generic template may attach to an instance: its requirements on
    /// the template's parameters hold under that instance's `bindings`. Requirements on the
    /// method's own type parameters are checked at each call instead.
    pub(in crate::analyzer) fn member_requirements_hold(
        &mut self,
        method: &FunctionNode<'a>,
        bindings: &GenericBindings,
    ) -> bool {
        member_requirements(&method.attributes)
            .into_iter()
            .all(|(parameter, member, _)| match bindings.get(&parameter) {
                Some(bound) => {
                    let ty = self.type_ctx.lower(bound);
                    self.type_provides_member(ty, &member, 0)
                }
                None => true,
            })
    }

    fn type_provides_member(&mut self, ty: TypeId, member: &str, depth: usize) -> bool {
        if depth > MAX_REQUIREMENT_DEPTH {
            return false;
        }
        if matches!(self.type_ctx.interner.kind(ty), TyKind::Error) {
            return true;
        }
        match self.template_member_requirements(ty, member) {
            Some(nested) => nested
                .into_iter()
                .all(|(ty, member)| self.type_provides_member(ty, &member, depth + 1)),
            None => false,
        }
    }

    /// The requirements the method `member` of `ty` places on `ty`'s type arguments, or `None`
    /// when `ty` has no such method.
    fn template_member_requirements(
        &mut self,
        ty: TypeId,
        member: &str,
    ) -> Option<Vec<(TypeId, String)>> {
        let (target, args) = match self.type_ctx.interner.kind(ty) {
            TyKind::Struct(def, args) | TyKind::Union(def, args) if !args.is_empty() => {
                (GenericExtendTarget::Nominal(*def), args.clone())
            }
            TyKind::Array(elem) => (GenericExtendTarget::Array, vec![*elem]),
            _ => {
                let declared = self.method_overloaded(ty, member)
                    || self.method_info(ty, member).is_ok()
                    || self
                        .function_table
                        .generic_method_overloads
                        .contains_key(&(ty, member.to_string()))
                    || self.declared_in_source(ty, member);
                return declared.then(Vec::new);
            }
        };
        let mut candidates: Vec<(&'a FunctionNode<'a>, Vec<String>)> = Vec::new();
        if let GenericExtendTarget::Nominal(def) = target {
            if let Some(template) = self.generic_struct(def) {
                let names = generic_param_names(&template.generic_parameters);
                candidates.extend(template.methods.iter().map(|m| (m, names.clone())));
            } else if let Some(template) = self.generic_union(def) {
                let names = generic_param_names(&template.generic_parameters);
                candidates.extend(template.methods.iter().map(|m| (m, names.clone())));
            }
        }
        for ext in self.generic_extends.get(&target).into_iter().flatten() {
            let names = generic_param_names(&ext.generic_parameters);
            candidates.extend(ext.methods.iter().map(|m| (m, names.clone())));
        }
        let (method, names) = candidates
            .into_iter()
            .find(|(method, _)| method.name.text == member)?;
        Some(
            member_requirements(&method.attributes)
                .into_iter()
                .filter_map(|(parameter, member, _)| {
                    let index = names.iter().position(|name| *name == parameter)?;
                    Some((*args.get(index)?, member))
                })
                .collect(),
        )
    }

    /// Whether a non-generic declaration or extension of `ty` declares `member`. Generic types can
    /// be instantiated before those methods are registered.
    fn declared_in_source(&mut self, ty: TypeId, member: &str) -> bool {
        let program = self.program;
        let scope = self.type_ctx.scope();
        let mut found = false;
        for ext in program.extends.iter() {
            if ext.generic_parameters.is_some()
                || !ext.methods.iter().any(|m| m.name.text == member)
            {
                continue;
            }
            self.type_ctx
                .set_scope(self.graph.module_for_file(ext.file_path.as_deref()));
            found = self.type_ctx.lower(&Self::type_from_name(&ext.target.text)) == ty;
            if found {
                break;
            }
        }
        self.type_ctx.set_scope(scope);
        found
            || program.structs.iter().any(|declaration| {
                declaration.generic_parameters.is_none()
                    && declaration.methods.iter().any(|m| m.name.text == member)
                    && matches!(
                        self.type_ctx.interner.kind(ty),
                        TyKind::Struct(def, _) if self.type_ctx.defs.name(*def) == declaration.name.text
                            && self.graph.module_for_file(declaration.file_path.as_deref()) == def.module
                    )
            })
    }
}
