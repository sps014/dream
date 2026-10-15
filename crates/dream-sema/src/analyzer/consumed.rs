use super::Analyzer;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::token::syntax_token::SyntaxToken;

impl Analyzer<'_> {
    /// Distinguishes removed functions from ordinary unresolved names, including imports.
    pub(super) fn report_consumed(
        &self,
        name: &SyntaxToken,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        let current = self
            .current_file
            .as_ref()
            .and_then(|p| self.graph.files.iter().find(|f| f.path == p.as_ref()))
            .map(|f| f.module);
        let module = current
            .map(|m| self.graph.modules[m.0 as usize].path.as_str())
            .unwrap_or_default();
        let mut id = if name.text.contains("::") {
            name.text.clone()
        } else {
            dream_abi::attributes::decl_identity(module, &name.text)
        };
        if !self.graph.consumed.contains_key(&id) {
            if let Some((import_module, item, _, _)) =
                self.aliased_imports.iter().find(|(_, _, alias, file)| {
                    alias.text == name.text && self.current_file.as_deref() == Some(file.as_str())
                })
            {
                id = dream_abi::attributes::decl_identity(import_module, item);
            } else if let Some((imported, _)) = self.graph.consumed.iter().find(|(candidate, _)| {
                let Some((owner, item)) = candidate.rsplit_once("::") else {
                    return false;
                };
                item == name.text
                    && current.is_some_and(|m| {
                        self.graph.modules[m.0 as usize]
                            .imports
                            .iter()
                            .any(|i| self.graph.modules[i.0 as usize].path == owner)
                    })
            }) {
                id = imported.clone();
            }
        }
        if let Some(by) = self.graph.consumed.get(&id) {
            diagnostics.report_error(
                format!(
                    "'{}' was consumed by generator '{}' and is not available as a function",
                    name.text, by.generator
                ),
                Some(name.position),
            );
            true
        } else {
            false
        }
    }
}
