use super::*;
use dream_syntax::nodes::Visibility;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn register_import_aliases(&mut self, diagnostics: &mut DiagnosticBag) {
        for (module_path, item, alias, importing_file) in std::mem::take(&mut self.aliased_imports) {
            let scope = self.graph.module_for_file(Some(&importing_file));
            self.type_ctx.set_scope(scope);
            diagnostics.file_path = Some(importing_file);
            let qualified = format!("{module_path}::{item}");
            let identities = self.function_candidates(&qualified);
            if identities.is_empty() {
                diagnostics.report_error(
                    format!("no item '{item}' found in module '{module_path}'"),
                    Some(alias.position),
                );
                continue;
            }
            if identities.iter().any(|key| self.function_table.functions.get(key).is_some_and(|info| info.visibility == Visibility::Private)) {
                diagnostics.report_error(
                    format!("'{item}' in module '{module_path}' is private; only 'public'/'internal' items can be imported with 'as'"),
                    Some(alias.position),
                );
                continue;
            }
            if let Err(error) = self.function_table.bind_alias(scope, &alias.text, identities) {
                diagnostics.report_error(error.to_string(), Some(alias.position));
            }
        }
    }
}
