//! Attribute types declared in Dream source (`@attribute struct name { ... }`): the embedded
//! stdlib's, collected once, plus the ones the open document declares or imports. Completion,
//! hover and signature help fall back to these when a name is not a builtin attribute.

use dream::syntax::nodes::{EnumDeclarationNode, StructDeclarationNode};
use dream_abi::attributes::{UserAttributeParam, UserAttributes};
use std::sync::OnceLock;

use super::Index;

#[derive(Debug, Clone)]
pub struct DeclaredAttribute {
    pub name: String,
    pub type_name: String,
    /// The std package declaring it; `None` when the program itself declares it.
    pub package: Option<&'static str>,
    /// `@name(field: type, ...)`, or `@name` for a field-less attribute.
    pub signature: String,
    pub doc: Option<String>,
    pub params: Vec<UserAttributeParam>,
    pub enum_choices: Vec<Vec<String>>,
}

impl DeclaredAttribute {
    pub fn hover(&self) -> String {
        let mut out = format!("```dream\n{}\n```", self.signature);
        if let Some(doc) = &self.doc {
            out.push_str("\n\n---\n\n");
            out.push_str(doc);
        }
        if let Some(package) = self.package {
            out.push_str(&format!(
                "\n\n*Declared in* `{package}` (`import {package};`)"
            ));
        }
        out
    }
}

/// The `//` comment lines directly above the first attribute of the declaration at `at`.
fn doc_above(source: &str, at: usize) -> Option<String> {
    let line_start = source[..at.min(source.len())]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let mut lines: Vec<&str> = source[..line_start]
        .lines()
        .rev()
        .map(str::trim)
        .take_while(|l| l.starts_with("//"))
        .map(|l| l.trim_start_matches('/').trim())
        .collect();
    lines.reverse();
    (!lines.is_empty()).then(|| lines.join(" "))
}

pub fn collect(
    source: &str,
    structs: &[StructDeclarationNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
    package: Option<&'static str>,
) -> Vec<DeclaredAttribute> {
    let mut diagnostics = dream::diagnostics::DiagnosticBag::new(None);
    let registry = UserAttributes::collect(
        structs,
        enums,
        &|_| package.unwrap_or("").to_string(),
        Default::default(),
        &mut diagnostics,
    );
    let mut out = Vec::new();
    for s in structs {
        if !s.attributes.iter().any(|a| a.name.text == "attribute") {
            continue;
        }
        let name = s.name.text.clone();
        let Some(contract) = registry.get(&name) else {
            continue;
        };
        let signature = contract.signature_label();
        let first = s
            .attributes
            .iter()
            .map(|a| a.name.position.start)
            .min()
            .unwrap_or(s.name.position.start);
        out.push(DeclaredAttribute {
            name: contract.name.clone(),
            type_name: contract.type_name.clone(),
            package,
            signature,
            doc: doc_above(source, first),
            params: contract.params.clone(),
            enum_choices: contract
                .params
                .iter()
                .map(|p| {
                    p.enum_type
                        .as_ref()
                        .map(|id| registry.enum_members(id).to_vec())
                        .unwrap_or_default()
                })
                .collect(),
        });
    }
    out
}

/// Every attribute type the embedded stdlib declares, whether or not the program imports its
/// package (completion then says which import it needs).
pub fn std_attributes() -> &'static [DeclaredAttribute] {
    static ALL: OnceLock<Vec<DeclaredAttribute>> = OnceLock::new();
    ALL.get_or_init(|| {
        dream_stdlib::attributes::all_specs()
            .map(|contract| DeclaredAttribute {
                name: contract.name.clone(),
                type_name: contract.type_name.clone(),
                package: dream_stdlib::find_package(&contract.module).map(|p| p.name),
                signature: contract.signature_label(),
                doc: Some(contract.doc.clone()),
                params: contract.params.clone(),
                enum_choices: contract
                    .params
                    .iter()
                    .map(|p| {
                        p.enum_type
                            .as_ref()
                            .map(|id| {
                                dream_stdlib::attributes::attribute_catalog()
                                    .enum_members(id)
                                    .to_vec()
                            })
                            .unwrap_or_default()
                    })
                    .collect(),
            })
            .collect()
    })
}

impl Index {
    /// Attribute types the document declares or imports first, then the stdlib's.
    pub fn declared_attributes(&self) -> impl Iterator<Item = &DeclaredAttribute> {
        self.attributes.iter().chain(std_attributes())
    }

    pub fn declared_attribute(&self, name: &str) -> Option<&DeclaredAttribute> {
        self.declared_attributes().find(|a| a.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_declares_json_with_docs() {
        let json = std_attributes()
            .iter()
            .find(|a| a.name == "json")
            .expect("system.json declares @json");
        assert_eq!(json.package, Some("system.json"));
        assert!(json.doc.as_deref().is_some_and(|d| d.contains("JSON")));
        let renamed = std_attributes()
            .iter()
            .find(|a| a.name == "property_name")
            .unwrap();
        assert_eq!(renamed.signature, "@property_name(name: string)");
    }
}
