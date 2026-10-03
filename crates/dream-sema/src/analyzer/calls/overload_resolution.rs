use super::super::*;
use crate::function_table::{FunctionTableInfo, OverloadResolution};

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn function_info(&self, name: &str) -> Result<FunctionTableInfo, crate::errors::SymbolError> {
        self.function_table.lookup(&self.type_ctx, name)
    }

    pub(in crate::analyzer) fn function_overloaded(&self, name: &str) -> bool {
        self.function_table.is_overloaded(&self.type_ctx, name)
    }

    pub(in crate::analyzer) fn function_candidates(&self, name: &str) -> Vec<crate::function_table::FunctionIdentity> {
        self.function_table.candidates(&self.type_ctx, name)
    }

    pub(in crate::analyzer) fn generic_function_template(&self, name: &str) -> Option<&'a FunctionNode<'a>> {
        let def = self.type_ctx.resolve(DefKind::Function, name)?;
        self.generic_functions.get(&def).copied()
    }

    pub(in crate::analyzer) fn function_declaration(&mut self, function: &FunctionNode<'_>) -> Option<crate::function_table::FunctionIdentity> {
        if let Some(identity) = self.function_table.declaration_node(function) { return Some(identity); }
        let params: Vec<_> = function.parameters.iter().map(|p| self.type_ctx.lower(&p.type_)).collect();
        self.function_table.declaration(&self.type_ctx, &function.name.text, &params)
    }

    pub(in crate::analyzer) fn type_id_display(&self, ty: dream_types::TypeId) -> String {
        dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, ty)
    }
    /// Resolves an overloaded base name against the concrete `arg_types`, returning the selected
    /// signature or a human-readable error (no match / ambiguous). Used by both free-function and
    /// method call analysis (methods prepend the receiver type as the implicit `this` argument).
    pub(crate) fn select_function_overload(
        &mut self,
        base: &str,
        arg_types: &[dream_types::TypeId],
    ) -> Result<FunctionTableInfo, String> {
        let candidates = self.function_candidates(base);
        self.select_candidate_overload(base, candidates, arg_types)
    }

    pub(in crate::analyzer) fn method_info(&self, owner: dream_types::TypeId, member: &str) -> Result<FunctionTableInfo, crate::errors::SymbolError> {
        let keys = self.function_table.method_candidates(owner, member);
        match keys.as_slice() {
            [key] => self.function_table.get_function(key),
            _ => Err(crate::errors::SymbolError::new(format!("Method does not resolve uniquely ({member})"))),
        }
    }

    /// The definition of `owner`'s only method named `member`, if it is not overloaded.
    pub(in crate::analyzer) fn unique_method_def(&self, owner: dream_types::TypeId, member: &str) -> Option<dream_types::DefId> {
        match self.function_table.method_candidates(owner, member).as_slice() {
            [(def, _)] => Some(*def),
            _ => None,
        }
    }

    pub(in crate::analyzer) fn method_overloaded(&self, owner: dream_types::TypeId, member: &str) -> bool {
        self.function_table.method_candidates(owner, member).len() > 1
    }

    pub(in crate::analyzer) fn select_method_overload(&mut self, owner: dream_types::TypeId, member: &str, args: &[dream_types::TypeId]) -> Result<FunctionTableInfo, String> {
        let label = format!("{}.{}", self.type_id_display(owner), member);
        self.select_candidate_overload(&label, self.function_table.method_candidates(owner, member), args)
    }

    fn select_candidate_overload(&mut self, base: &str, candidates: Vec<crate::function_table::FunctionIdentity>, arg_types: &[dream_types::TypeId]) -> Result<FunctionTableInfo, String> {
        let mut compatibility = indexmap::IndexMap::new();
        for key in &candidates {
            let Ok(info) = self.function_table.get_function(key) else { continue; };
            let mut parameters = info.parameters;
            if info.is_variadic {
                if let Some(last) = parameters.last() {
                    if let dream_types::TyKind::Array(element) = self.type_ctx.interner.kind(*last) {
                        parameters.push(*element);
                    }
                }
            }
            for param in parameters {
                for &arg in arg_types {
                    let mut sink = DiagnosticBag::new(None);
                    let compatible = dream_types::overload_compatible(&self.type_ctx.interner, param, arg)
                        || self.value_type_assignable(param, arg, &mut sink);
                    compatibility.insert((param, arg), compatible);
                }
            }
        }
        match self.function_table.select_candidates(&self.type_ctx, candidates, arg_types, |param, arg| compatibility.get(&(param, arg)).copied().unwrap_or(false)) {
            OverloadResolution::Unique(key) => match self.function_table.get_function(&key) {
                Ok(info) => Ok(info),
                Err(_) => Err(format!("Could not resolve function '{base}'")),
            },
            OverloadResolution::None => {
                let pretty: Vec<String> =
                    arg_types.iter().map(|t| self.type_id_display(*t)).collect();
                Err(format!(
                    "No overload of '{}' matches argument types ({})",
                    base,
                    pretty.join(", ")
                ))
            }
            OverloadResolution::Ambiguous(keys) => {
                let pretty: Vec<String> =
                    arg_types.iter().map(|t| self.type_id_display(*t)).collect();
                Err(format!(
                    "Ambiguous call to '{}' with argument types ({}); candidates: {}",
                    base,
                    pretty.join(", "),
                    keys.iter().filter_map(|key| self.function_table.functions.get(key)).map(|info| info.name.clone()).collect::<Vec<_>>().join(", ")
                ))
            }
        }
    }

    pub(crate) fn validate_arguments(
        &mut self,
        error_prefix: &str,
        expected: &[dream_types::TypeId],
        given: &[dream_types::TypeId],
        position: dream_text::text_span::TextSpan,
        diagnostics: &mut dream_diagnostics::DiagnosticBag,
    ) {
        for (i, given_type) in given.iter().enumerate() {
            if let Some(expected_type_str) = expected.get(i) {
                if !self.value_type_assignable(*expected_type_str, *given_type, diagnostics) {
                    let expected_pretty = self.type_id_display(*expected_type_str);
                    let given_pretty = self.type_id_display(*given_type);
                    diagnostics.report_error(
                        format!(
                            "{} expects parameter {} to be {}, got {}",
                            error_prefix,
                            i + 1,
                            expected_pretty,
                            given_pretty
                        ),
                        Some(position),
                    );
                }
            }
        }
    }
}
