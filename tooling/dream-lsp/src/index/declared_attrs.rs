//! Attribute types declared in Dream source (`@attribute struct name { ... }`): the embedded
//! stdlib's, collected once, plus the ones the open document declares or imports. Completion,
//! hover and signature help fall back to these when a name is not a builtin attribute.

use bumpalo::Bump;
use dream::diagnostics::DiagnosticBag;
use dream::syntax::lexer::Lexer;
use dream::syntax::nodes::StructDeclarationNode;
use dream::syntax::parser::Parser;
use dream_abi::attributes::has_attribute_decl_attr;
use std::sync::OnceLock;

use super::Index;

#[derive(Debug, Clone)]
pub struct DeclaredAttribute {
    pub name: String,
    /// The std package declaring it; `None` when the program itself declares it.
    pub package: Option<&'static str>,
    /// `@name(field: type, ...)`, or `@name` for a field-less attribute.
    pub signature: String,
    pub doc: Option<String>,
}

impl DeclaredAttribute {
    pub fn hover(&self) -> String {
        let mut out = format!("```dream\n{}\n```", self.signature);
        if let Some(doc) = &self.doc {
            out.push_str("\n\n---\n\n");
            out.push_str(doc);
        }
        if let Some(package) = self.package {
            out.push_str(&format!("\n\n*Declared in* `{package}` (`import {package};`)"));
        }
        out
    }
}

/// The `//` comment lines directly above the first attribute of the declaration at `at`.
fn doc_above(source: &str, at: usize) -> Option<String> {
    let line_start = source[..at.min(source.len())].rfind('\n').map_or(0, |i| i + 1);
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
    package: Option<&'static str>,
) -> Vec<DeclaredAttribute> {
    let mut out = Vec::new();
    for s in structs {
        if !has_attribute_decl_attr(&s.attributes) {
            continue;
        }
        let name = s.name.text.clone();
        let fields: Vec<String> = s
            .fields
            .iter()
            .map(|f| format!("{}: {}", f.name.text, f.field_type.display_name()))
            .collect();
        let signature = if fields.is_empty() {
            format!("@{name}")
        } else {
            format!("@{name}({})", fields.join(", "))
        };
        let first = s
            .attributes
            .iter()
            .map(|a| a.name.position.start)
            .min()
            .unwrap_or(s.name.position.start);
        out.push(DeclaredAttribute {
            name,
            package,
            signature,
            doc: doc_above(source, first),
        });
    }
    out
}

/// Every attribute type the embedded stdlib declares, whether or not the program imports its
/// package (completion then says which import it needs).
pub fn std_attributes() -> &'static [DeclaredAttribute] {
    static ALL: OnceLock<Vec<DeclaredAttribute>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut out = Vec::new();
        for pkg in dream_stdlib::STD_PACKAGES {
            for &(_, source) in pkg.files {
                if !source.contains("@attribute") {
                    continue;
                }
                let arena = Bump::new();
                let mut scratch = DiagnosticBag::new(None);
                let mut parser = Parser::new(Lexer::new(source.to_string()), &arena, &mut scratch);
                if let Ok(ast) = parser.parse() {
                    out.extend(collect(source, &ast.get_root().structs, Some(pkg.name)));
                }
            }
        }
        out
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
