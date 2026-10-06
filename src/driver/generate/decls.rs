//! Declaration identities over the merged (pre-sema) program: `module::Name[.member]`, the same
//! spelling `declof(path)` folds to in the analyzer.

use crate::driver::source_loader::ProgramAccumulator;
use dream_abi::attributes::decl_identity;
use dream_text::text_span::TextSpan;
use indexmap::IndexMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Class,
    Struct,
    Union,
    Enum,
    Interface,
    Function,
}

impl DeclKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DeclKind::Class => "class",
            DeclKind::Struct => "struct",
            DeclKind::Union => "union",
            DeclKind::Enum => "enum",
            DeclKind::Interface => "interface",
            DeclKind::Function => "function",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeclEntry {
    pub id: String,
    pub name: String,
    pub module: String,
    pub kind: DeclKind,
    pub file: Option<String>,
    /// Member names (fields, methods, variants) that `Type.member` paths may name.
    pub members: Vec<String>,
}

/// Every top-level declaration, keyed by bare name (first declaration wins, matching the
/// analyzer's global type namespace).
#[derive(Debug, Default)]
pub struct DeclIndex {
    by_name: IndexMap<String, DeclEntry>,
    functions: IndexMap<String, DeclEntry>,
}

pub fn module_of(acc: &ProgramAccumulator<'_>, file: Option<&str>) -> String {
    file.and_then(|f| acc.file_modules.get(f))
        .map(|m| m.to_string())
        .unwrap_or_default()
}

impl DeclIndex {
    pub fn build(acc: &ProgramAccumulator<'_>) -> Self {
        let mut index = DeclIndex::default();
        let mut add = |name: &str, kind, file: Option<&str>, members: Vec<String>| {
            let module = module_of(acc, file);
            index
                .by_name
                .entry(name.to_string())
                .or_insert_with(|| DeclEntry {
                    id: decl_identity(&module, name),
                    name: name.to_string(),
                    module,
                    kind,
                    file: file.map(str::to_string),
                    members,
                });
        };
        for s in &acc.all_structs {
            let mut members: Vec<String> = s.fields.iter().map(|f| f.name.text.clone()).collect();
            members.extend(s.methods.iter().map(|m| m.name.text.clone()));
            let kind = if s.is_value {
                DeclKind::Struct
            } else {
                DeclKind::Class
            };
            add(&s.name.text, kind, s.file_path.as_deref(), members);
        }
        for e in &acc.all_enums {
            let mut members: Vec<String> = e.variants.iter().map(|v| v.name.text.clone()).collect();
            members.extend(e.methods.iter().map(|m| m.name.text.clone()));
            let kind = if e.is_data_enum() {
                DeclKind::Union
            } else {
                DeclKind::Enum
            };
            add(&e.name.text, kind, e.file_path.as_deref(), members);
        }
        for i in &acc.all_interfaces {
            let members = i.methods.iter().map(|m| m.name.text.clone()).collect();
            add(
                &i.name.text,
                DeclKind::Interface,
                i.file_path.as_deref(),
                members,
            );
        }
        for ext in &acc.all_extends {
            if let Some(entry) = index.by_name.get_mut(&ext.target.text) {
                entry
                    .members
                    .extend(ext.methods.iter().map(|m| m.name.text.clone()));
            }
        }
        for f in &acc.all_functions {
            let module = module_of(acc, f.file_path.as_deref());
            index
                .functions
                .entry(f.name.text.clone())
                .or_insert_with(|| DeclEntry {
                    id: decl_identity(&module, &f.name.text),
                    name: f.name.text.clone(),
                    module,
                    kind: DeclKind::Function,
                    file: f.file_path.as_deref().map(str::to_string),
                    members: Vec::new(),
                });
        }
        index
    }

    pub fn type_decl(&self, name: &str) -> Option<&DeclEntry> {
        self.by_name.get(name)
    }

    pub fn types(&self) -> impl Iterator<Item = &DeclEntry> {
        self.by_name.values()
    }

    /// Resolves a dotted path (`Type`, `Type.member`, `func`, optionally module-qualified)
    /// exactly like `declof`.
    pub fn resolve_path(&self, parts: &[&str]) -> Option<String> {
        for split in 0..parts.len() {
            let module = parts[..split].join(".");
            let name = parts[split];
            let rest = &parts[split + 1..];
            if rest.len() > 1 {
                continue;
            }
            let module_ok = |e: &DeclEntry| split == 0 || e.module == module;
            if let Some(e) = self.by_name.get(name).filter(|e| module_ok(e)) {
                return match rest.first() {
                    None => Some(e.id.clone()),
                    Some(m) if e.members.iter().any(|x| x == m) => Some(format!("{}.{m}", e.id)),
                    Some(_) => None,
                };
            }
            if rest.is_empty()
                && let Some(e) = self.functions.get(name).filter(|e| module_ok(e)) {
                    return Some(e.id.clone());
                }
        }
        None
    }
}

/// Source span of a snapshot identity, for mapping generator diagnostics back into user files.
#[derive(Debug, Default)]
pub struct IdSpans {
    spans: IndexMap<String, (Option<String>, TextSpan)>,
}

impl IdSpans {
    pub fn insert(&mut self, id: &str, file: Option<&str>, span: TextSpan) {
        self.spans
            .entry(id.to_string())
            .or_insert((file.map(str::to_string), span));
    }

    pub fn get(&self, id: &str) -> Option<(Option<&str>, TextSpan)> {
        self.spans.get(id).map(|(f, s)| (f.as_deref(), *s))
    }
}
