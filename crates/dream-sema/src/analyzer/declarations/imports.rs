use super::*;
use dream_syntax::nodes::Visibility;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn register_type_import_aliases(
        &mut self,
        diagnostics: &mut DiagnosticBag,
    ) {
        for (module_path, item, alias, importing_file) in &self.aliased_imports {
            let scope = self.graph.module_for_file(Some(importing_file));
            self.type_ctx.set_scope(scope);
            let qualified = format!("{module_path}::{item}");
            let Some(kind) = self.type_ctx.nominal_kind(&qualified) else {
                continue;
            };
            let Some(def) = self.type_ctx.resolve(kind, &qualified) else {
                continue;
            };
            let visibility = self
                .graph
                .files
                .iter()
                .filter(|file| self.type_ctx.module_path(file.module) == Some(module_path.as_str()))
                .find_map(|file| {
                    file.program
                        .structs
                        .iter()
                        .find(|d| d.name.text == *item)
                        .map(|d| d.visibility)
                        .or_else(|| {
                            file.program
                                .enums
                                .iter()
                                .find(|d| d.name.text == *item)
                                .map(|d| d.visibility)
                        })
                        .or_else(|| {
                            file.program
                                .interfaces
                                .iter()
                                .find(|d| d.name.text == *item)
                                .map(|d| d.visibility)
                        })
                });
            diagnostics.file_path = Some(importing_file.clone());
            if visibility == Some(Visibility::Private)
                && !self.trusted_generated_file(importing_file.as_str())
                && !self.generator_can_access(&qualified, Some(&Rc::from(importing_file.as_str())))
            {
                diagnostics.report_error(format!("'{item}' in module '{module_path}' is private; only 'public'/'internal' items can be imported with 'as'"), Some(alias.position));
                continue;
            }
            if let Err(error) = self.type_ctx.bind_type_alias(scope, &alias.text, def) {
                diagnostics.report_error(error, Some(alias.position));
            }
        }
    }

    pub(in crate::analyzer) fn register_import_aliases(&mut self, diagnostics: &mut DiagnosticBag) {
        for (module_path, item, alias, importing_file) in std::mem::take(&mut self.aliased_imports)
        {
            let scope = self.graph.module_for_file(Some(&importing_file));
            self.type_ctx.set_scope(scope);
            let generated = self.trusted_generated_file(importing_file.as_str());
            diagnostics.file_path = Some(importing_file);
            let qualified = format!("{module_path}::{item}");
            if self.type_ctx.nominal_kind(&qualified).is_some() {
                continue;
            }
            let identities = self.function_candidates(&qualified);
            if identities.is_empty() {
                diagnostics.report_error(
                    format!("no item '{item}' found in module '{module_path}'"),
                    Some(alias.position),
                );
                continue;
            }
            if !generated
                && !self.generator_can_access(
                    &qualified,
                    diagnostics.file_path.as_deref().map(Rc::from).as_ref(),
                )
                && identities.iter().any(|key| {
                    self.function_table
                        .functions
                        .get(key)
                        .is_some_and(|info| info.visibility == Visibility::Private)
                })
            {
                diagnostics.report_error(
                    format!("'{item}' in module '{module_path}' is private; only 'public'/'internal' items can be imported with 'as'"),
                    Some(alias.position),
                );
                continue;
            }
            if let Err(error) = self
                .function_table
                .bind_alias(scope, &alias.text, identities)
            {
                diagnostics.report_error(error.to_string(), Some(alias.position));
            }
        }
    }
}
