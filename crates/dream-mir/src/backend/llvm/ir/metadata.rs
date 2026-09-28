//! Numbered metadata (`!N = ...`), deduplicated by body text so structurally identical nodes
//! share one number and output stays deterministic.

use indexmap::IndexMap;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MdRef(pub u32);

impl fmt::Display for MdRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "!{}", self.0)
    }
}

#[derive(Default)]
pub struct Metadata {
    nodes: IndexMap<String, MdRef>,
    /// `distinct` nodes are never deduplicated.
    ordered: Vec<(MdRef, String)>,
    named: IndexMap<String, Vec<MdRef>>,
    module_flags: Vec<MdRef>,
}

impl Metadata {
    /// A uniqued node; `body` is the text after `!N = ` (e.g. `!{!"dream tbaa"}`).
    pub fn node(&mut self, body: impl Into<String>) -> MdRef {
        let body = body.into();
        if let Some(r) = self.nodes.get(&body) {
            return *r;
        }
        let r = MdRef(self.ordered.len() as u32);
        self.nodes.insert(body.clone(), r);
        self.ordered.push((r, body));
        r
    }

    pub fn distinct(&mut self, body: impl Into<String>) -> MdRef {
        let r = MdRef(self.ordered.len() as u32);
        self.ordered.push((r, format!("distinct {}", body.into())));
        r
    }

    /// Allocates a number now and fills the body later (self-referential nodes such as alias
    /// domains, `distinct !{!N}`).
    pub fn reserve(&mut self) -> MdRef {
        let r = MdRef(self.ordered.len() as u32);
        self.ordered.push((r, String::new()));
        r
    }

    pub fn fill(&mut self, r: MdRef, body: impl Into<String>) {
        self.ordered[r.0 as usize].1 = body.into();
    }

    pub fn tuple(&mut self, items: &[MdRef]) -> MdRef {
        let body = format!(
            "!{{{}}}",
            items
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.node(body)
    }

    pub fn string(s: &str) -> String {
        format!("!\"{}\"", s.replace('\\', "\\5C").replace('"', "\\22"))
    }

    pub fn add_named(&mut self, name: &str, r: MdRef) {
        self.named.entry(name.to_string()).or_default().push(r);
    }

    /// `!llvm.module.flags` entry: `behavior` is the LLVM merge behavior (1 = error, 2 = warning,
    /// 7 = max).
    pub fn module_flag(&mut self, behavior: u32, key: &str, value: &str) {
        let r = self.node(format!("!{{i32 {behavior}, !\"{key}\", {value}}}"));
        self.module_flags.push(r);
    }

    pub fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }

    pub fn write(&self, out: &mut String) {
        if !self.module_flags.is_empty() {
            out.push_str("!llvm.module.flags = !{");
            out.push_str(
                &self
                    .module_flags
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push_str("}\n");
        }
        for (name, refs) in &self.named {
            out.push_str(&format!(
                "!{} = !{{{}}}\n",
                name,
                refs.iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for (r, body) in &self.ordered {
            out.push_str(&format!("{r} = {body}\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_nodes_share_a_number() {
        let mut md = Metadata::default();
        let a = md.node("!{!\"x\"}");
        let b = md.node("!{!\"x\"}");
        let c = md.distinct("!{}");
        assert_eq!(a, b);
        assert_ne!(a, c);
        let mut out = String::new();
        md.write(&mut out);
        assert_eq!(out, "!0 = !{!\"x\"}\n!1 = distinct !{}\n");
    }
}
