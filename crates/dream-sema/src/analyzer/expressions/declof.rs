//! `declof(path)` — the generator-model identity of a declaration (`module::Type.member`).

use super::*;
use crate::errors::SemanticError;
use dream_diagnostics::DiagnosticBag;
use dream_hir::{HExpr, HExprKind};
use dream_syntax::token::syntax_token::SyntaxToken;

impl<'a> Analyzer<'a> {
    /// Resolves `declof(Type)`, `declof(Type.member)`, `declof(func)` or a module-qualified form
    /// (`declof(system.json.Json.serialize)`) to the `module::Name[.member]` string the generator
    /// snapshot uses for `GenDeclId`. Unresolved paths are compile errors.
    pub(in crate::analyzer) fn analyze_declof(
        &mut self,
        parts: &[SyntaxToken],
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let Some(first) = parts.first() else {
            self.hir_none();
            let _ = report(
                diagnostics,
                "declof requires a declaration path".to_string(),
                None,
            );
            return Ok(Type::Unknown);
        };
        let texts: Vec<&str> = parts.iter().map(|t| t.text.as_str()).collect();
        match self.resolve_decl_identity(&texts) {
            Some(id) => {
                let ty_id = self.type_ctx.interner.string();
                self.hir_set_last(Some(HExpr::new(ty_id, HExprKind::StringLit(id))));
                Ok(Self::type_from_name("string"))
            }
            None => {
                self.hir_none();
                let _ = report(
                    diagnostics,
                    format!(
                        "declof: '{}' does not name a type, function, field, method or variant",
                        texts.join(".")
                    ),
                    Some(first.position),
                );
                Ok(Type::Unknown)
            }
        }
    }

    fn resolve_decl_identity(&self, parts: &[&str]) -> Option<String> {
        for split in 0..parts.len() {
            let module = parts[..split].join(".");
            let name = parts[split];
            let rest = &parts[split + 1..];
            if rest.len() > 1 {
                continue;
            }
            let module_ok = |file: &Option<Rc<str>>| {
                split == 0 || self.module_of(file.as_ref()).as_deref() == Some(module.as_str())
            };
            let decl_module = |file: &Option<Rc<str>>| {
                self.module_of(file.as_ref())
                    .map(|m| m.to_string())
                    .unwrap_or_default()
            };
            let identity = |file: &Option<Rc<str>>| {
                let base = dream_abi::attributes::decl_identity(&decl_module(file), name);
                match rest.first() {
                    Some(member) => format!("{base}.{member}"),
                    None => base,
                }
            };
            let extend_has = |member: &str| {
                self.program.extends.iter().any(|e| {
                    e.target.text == name && e.methods.iter().any(|m| m.name.text == member)
                })
            };
            if let Some(s) = self
                .program
                .structs
                .iter()
                .find(|s| s.name.text == name && module_ok(&s.file_path))
            {
                let ok = rest.first().is_none_or(|m| {
                    s.fields.iter().any(|f| f.name.text == *m)
                        || s.methods.iter().any(|f| f.name.text == *m)
                        || extend_has(m)
                });
                return ok.then(|| identity(&s.file_path));
            }
            if let Some(e) = self
                .program
                .enums
                .iter()
                .find(|e| e.name.text == name && module_ok(&e.file_path))
            {
                let ok = rest.first().is_none_or(|m| {
                    e.variants.iter().any(|v| v.name.text == *m)
                        || e.methods.iter().any(|f| f.name.text == *m)
                        || extend_has(m)
                });
                return ok.then(|| identity(&e.file_path));
            }
            if let Some(i) = self
                .program
                .interfaces
                .iter()
                .find(|i| i.name.text == name && module_ok(&i.file_path))
            {
                let ok = rest
                    .first()
                    .is_none_or(|m| i.methods.iter().any(|f| f.name.text == *m));
                return ok.then(|| identity(&i.file_path));
            }
            if rest.is_empty()
                && let Some(f) = self
                    .program
                    .functions
                    .iter()
                    .find(|f| f.name.text == name && module_ok(&f.file_path))
            {
                return Some(identity(&f.file_path));
            }
        }
        None
    }
}
