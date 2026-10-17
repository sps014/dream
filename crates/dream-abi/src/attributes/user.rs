//! Attribute contracts are ordinary Dream declarations; only their compiler effects live in Rust.

use super::*;
use dream_syntax::nodes::Visibility;
use std::collections::BTreeMap;

pub const JSON_ATTRIBUTE_ID: &str = "system.json::JsonAttribute";
pub const ATTRIBUTE_TARGET_ENUM: &str = "AttributeTarget";

pub fn decl_identity(module: &str, name: &str) -> String {
    format!("{module}::{name}")
}

/// The name an `@attribute` type is written with: `ShaderAttribute` is used as `@shader`.
pub fn attribute_use_name(type_name: &str) -> String {
    use heck::ToSnakeCase;
    type_name
        .strip_suffix("Attribute")
        .filter(|name| !name.is_empty())
        .map(str::to_snake_case)
        .unwrap_or_else(|| type_name.to_string())
}

#[derive(Debug, Clone)]
pub struct UserAttributeParam {
    pub name: String,
    pub kind: ArgKind,
    pub ty: String,
    pub enum_type: Option<String>,
    pub variadic: bool,
    pub optional: bool,
    pub array: bool,
}

#[derive(Debug, Clone)]
pub struct UserAttribute {
    pub id: String,
    pub name: String,
    pub type_name: String,
    pub module: String,
    pub file: Option<String>,
    pub visibility: Visibility,
    pub targets: Vec<AttributeTarget>,
    pub params: Vec<UserAttributeParam>,
    pub repeatable: bool,
    pub doc: String,
}

impl UserAttribute {
    pub fn required(&self) -> usize {
        self.params
            .iter()
            .filter(|p| !p.variadic && !p.optional)
            .count()
    }

    pub fn accepts_any_count(&self) -> bool {
        self.params.last().is_some_and(|p| p.variadic)
    }

    pub fn param_labels(&self) -> Vec<String> {
        self.params
            .iter()
            .map(|p| {
                let suffix = if p.variadic {
                    "..."
                } else if p.optional {
                    "?"
                } else {
                    ""
                };
                format!("{}: {}{suffix}", p.name, p.ty)
            })
            .collect()
    }

    pub fn signature_label(&self) -> String {
        if self.params.is_empty() {
            format!("@{}", self.name)
        } else {
            format!("@{}({})", self.name, self.param_labels().join(", "))
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct AttributeEnum {
    pub module: String,
    pub file: Option<String>,
    pub visibility: Visibility,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UserAttributes {
    pub(super) by_id: BTreeMap<String, UserAttribute>,
    pub(super) enums: BTreeMap<String, AttributeEnum>,
    pub(super) modules: BTreeMap<String, String>,
    pub(super) imports: BTreeMap<String, Vec<String>>,
    import_hint: Option<fn(&str) -> Option<String>>,
    internal_callers: std::collections::BTreeSet<String>,
}

impl UserAttributes {
    pub fn with_internal_callers(mut self, files: std::collections::BTreeSet<String>) -> Self {
        self.internal_callers = files;
        self
    }

    pub fn with_import_hint(mut self, hint: fn(&str) -> Option<String>) -> Self {
        self.import_hint = Some(hint);
        self
    }

    pub fn with_visibility(
        mut self,
        modules: BTreeMap<String, String>,
        imports: BTreeMap<String, Vec<String>>,
    ) -> Self {
        self.modules = modules;
        self.imports = imports;
        self
    }

    pub(super) fn import_hint(&self, name: &str) -> Option<String> {
        self.import_hint.and_then(|hint| hint(name))
    }

    pub fn enum_members(&self, enum_type: &str) -> &[String] {
        self.enums
            .get(enum_type)
            .map(|e| e.members.as_slice())
            .unwrap_or_default()
    }

    pub fn get(&self, name: &str) -> Option<&UserAttribute> {
        let mut matches = self
            .by_id
            .values()
            .filter(|a| a.name == name || a.type_name == name || a.id == name);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    pub fn iter(&self) -> impl Iterator<Item = &UserAttribute> {
        self.by_id.values()
    }

    pub fn resolve(&self, name: &str, file: Option<&str>) -> Vec<&UserAttribute> {
        self.by_id
            .values()
            .filter(|a| {
                a.name == name && self.visible(&a.module, a.file.as_deref(), a.visibility, file)
            })
            .collect()
    }

    pub(super) fn visible(
        &self,
        module: &str,
        declaration_file: Option<&str>,
        visibility: Visibility,
        file: Option<&str>,
    ) -> bool {
        if self.modules.is_empty() {
            return true;
        }
        let own = file
            .and_then(|f| self.modules.get(f))
            .map(String::as_str)
            .unwrap_or("");
        match visibility {
            Visibility::Private => declaration_file == file,
            Visibility::Internal => {
                module == own || file.is_some_and(|f| self.internal_callers.contains(f))
            }
            Visibility::Public => {
                module == own
                    || matches!(module, "system.core" | "system.primitives")
                    || file
                        .and_then(|f| self.imports.get(f))
                        .is_some_and(|imports| imports.iter().any(|m| m == module))
            }
        }
    }

    pub(super) fn validate_use(
        &self,
        decl: &UserAttribute,
        attr: &AttributeNode,
        target: AttributeTarget,
        diagnostics: &mut DiagnosticBag,
    ) {
        use super::validate::{arg_matches_kind, kind_name};
        let name = &decl.name;
        let _ = attr.resolved_id.set(decl.id.clone());
        if !decl.targets.is_empty() && !decl.targets.contains(&target) {
            diagnostics.report_error(
                format!("'@{name}' cannot be applied to {}", target.display_name()),
                Some(attr.name.position),
            );
        }
        let count = attr.args.len();
        if decl.params.is_empty() && count > 0 {
            diagnostics.report_error(
                format!("'@{name}' does not take any arguments"),
                Some(attr.name.position),
            );
            return;
        }
        if count < decl.required() || (!decl.accepts_any_count() && count > decl.params.len()) {
            let expected = if decl.accepts_any_count() {
                format!("at least {}", decl.required())
            } else if decl.required() == decl.params.len() {
                decl.required().to_string()
            } else {
                format!("{}-{}", decl.required(), decl.params.len())
            };
            diagnostics.report_error(
                format!("'@{name}' expects {expected} argument(s), got {count}"),
                Some(attr.name.position),
            );
            return;
        }
        for (i, arg) in attr.args.iter().enumerate() {
            let Some(param) = decl.params.get(i.min(decl.params.len().saturating_sub(1))) else {
                break;
            };
            if !arg_matches_kind(arg, param.kind) {
                diagnostics.report_error(
                    format!(
                        "'@{name}' argument {} must be {}, got '{}'",
                        i + 1,
                        kind_name(param.kind),
                        arg.display()
                    ),
                    Some(arg.position()),
                );
                continue;
            }
            if let (AttributeArg::Enum(path), Some(enum_id)) = (arg, &param.enum_type) {
                let member = path.last().map(|p| p.text.as_str());
                let owner = path[..path.len().saturating_sub(1)]
                    .iter()
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let expected_name = enum_id.rsplit_once("::").map(|(_, n)| n).unwrap_or(enum_id);
                let candidates: Vec<_> = self
                    .enums
                    .iter()
                    .filter(|(id, e)| {
                        let n = id.rsplit_once("::").map(|(_, n)| n).unwrap_or(id);
                        (owner == n || owner == format!("{}.{}", e.module, n))
                            && self.visible(
                                &e.module,
                                e.file.as_deref(),
                                e.visibility,
                                diagnostics.file_path.as_deref(),
                            )
                    })
                    .collect();
                let ok = candidates.len() == 1
                    && candidates[0].0 == enum_id
                    && member.is_some_and(|m| candidates[0].1.members.iter().any(|v| v == m));
                if !ok {
                    diagnostics.report_error(
                        format!(
                            "'@{name}' argument {} must be a member of '{expected_name}', got '{}'",
                            i + 1,
                            arg.display()
                        ),
                        Some(arg.position()),
                    );
                }
            }
        }
    }
}
