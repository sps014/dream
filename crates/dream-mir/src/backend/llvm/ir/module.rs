//! A whole `.ll` module: header, named types, globals, declarations, definitions, metadata.
//! All tables are insertion-ordered so two emissions of the same MIR are byte-identical.

use super::attrs::{join, CallConv, FnAttr, Linkage, ParamAttr};
use super::fmt;
use super::function::FunctionWriter;
use super::metadata::Metadata;
use super::ty::{FnTy, Ty};
use indexmap::{IndexMap, IndexSet};

pub struct GlobalDef {
    pub linkage: Linkage,
    pub thread_local: bool,
    pub constant: bool,
    pub unnamed_addr: bool,
    pub ty: Ty,
    /// Initializer text (`zeroinitializer`, `{ i32 0, ... }`); `None` declares an external.
    pub init: Option<String>,
    pub align: u32,
}

pub struct Decl {
    pub fty: FnTy,
    pub ret_attrs: Vec<ParamAttr>,
    pub param_attrs: Vec<Vec<ParamAttr>>,
    pub attrs: Vec<FnAttr>,
    pub cc: CallConv,
}

pub struct ModuleWriter {
    pub source_name: String,
    pub triple: String,
    pub datalayout: String,
    types: IndexMap<String, Ty>,
    globals: IndexMap<String, GlobalDef>,
    decls: IndexMap<String, Decl>,
    defined: IndexSet<String>,
    bodies: String,
    pub md: Metadata,
}

impl ModuleWriter {
    pub fn new(source_name: &str, triple: &str, datalayout: &str) -> Self {
        Self {
            source_name: source_name.into(),
            triple: triple.into(),
            datalayout: datalayout.into(),
            types: IndexMap::new(),
            globals: IndexMap::new(),
            decls: IndexMap::new(),
            defined: IndexSet::new(),
            bodies: String::new(),
            md: Metadata::default(),
        }
    }

    pub fn named_struct(&mut self, name: &str, body: Ty) -> Ty {
        self.types.entry(name.to_string()).or_insert(body);
        Ty::Named(name.to_string())
    }

    pub fn has_global(&self, name: &str) -> bool {
        self.globals.contains_key(name)
    }

    pub fn global(&mut self, name: &str, def: GlobalDef) {
        self.globals.insert(name.to_string(), def);
    }

    /// First declaration wins; a later `define` of the same name suppresses it.
    pub fn declare(&mut self, name: &str, decl: Decl) {
        self.decls.entry(name.to_string()).or_insert(decl);
    }

    pub fn declared(&self, name: &str) -> Option<&Decl> {
        self.decls.get(name)
    }

    pub fn is_defined(&self, name: &str) -> bool {
        self.defined.contains(name)
    }

    pub fn define(&mut self, f: FunctionWriter) {
        assert!(
            self.defined.insert(f.name().to_string()),
            "ICE: LLVM function {} defined twice",
            f.name()
        );
        f.write(&mut self.bodies);
    }

    pub fn finish(self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "; ModuleID = '{}'\nsource_filename = \"{}\"\n",
            self.source_name,
            self.source_name.replace('\\', "\\5C").replace('"', "\\22")
        ));
        if !self.datalayout.is_empty() {
            out.push_str(&format!("target datalayout = \"{}\"\n", self.datalayout));
        }
        if !self.triple.is_empty() {
            out.push_str(&format!("target triple = \"{}\"\n", self.triple));
        }
        out.push('\n');
        for (name, body) in &self.types {
            out.push_str(&format!("%{} = type {body}\n", fmt::ident(name)));
        }
        if !self.types.is_empty() {
            out.push('\n');
        }
        for (name, g) in &self.globals {
            let init = match &g.init {
                Some(i) => format!(" {i}"),
                None => String::new(),
            };
            let linkage = if g.init.is_none() {
                "external ".to_string()
            } else {
                g.linkage.prefix().to_string()
            };
            out.push_str(&format!(
                "{} = {linkage}{}{}{} {}{init}, align {}\n",
                fmt::global(name),
                if g.thread_local { "thread_local " } else { "" },
                if g.unnamed_addr { "unnamed_addr " } else { "" },
                if g.constant { "constant" } else { "global" },
                g.ty,
                g.align
            ));
        }
        if !self.globals.is_empty() {
            out.push('\n');
        }
        for (name, d) in &self.decls {
            if self.defined.contains(name) {
                continue;
            }
            let params = d
                .fty
                .params
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let a = d.param_attrs.get(i).map(|a| join(a)).unwrap_or_default();
                    format!("{t}{a}")
                })
                .chain(d.fty.varargs.then(|| "...".to_string()))
                .collect::<Vec<_>>()
                .join(", ");
            let ret = if d.ret_attrs.is_empty() {
                d.fty.ret.to_string()
            } else {
                format!("{} {}", join(&d.ret_attrs).trim_start(), d.fty.ret)
            };
            out.push_str(&format!(
                "declare {}{ret} {}({params}){}\n",
                d.cc.prefix(),
                fmt::global(name),
                join(&d.attrs)
            ));
        }
        if !self.decls.is_empty() {
            out.push('\n');
        }
        out.push_str(&self.bodies);
        self.md.write(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_yield_to_definitions() {
        let mut m = ModuleWriter::new("t.dream", "arm64-apple-macosx", "");
        m.declare(
            "f",
            Decl {
                fty: FnTy::new(Ty::Void, vec![]),
                ret_attrs: vec![],
                param_attrs: vec![],
                attrs: vec![],
                cc: CallConv::C,
            },
        );
        m.declare(
            "puts",
            Decl {
                fty: FnTy::new(Ty::I32, vec![Ty::Ptr]),
                ret_attrs: vec![],
                param_attrs: vec![vec![ParamAttr::NoUndef]],
                attrs: vec![FnAttr::NoUnwind],
                cc: CallConv::C,
            },
        );
        let mut f = FunctionWriter::new("f", Ty::Void, vec![]);
        f.ret(None);
        m.define(f);
        m.global(
            "s",
            GlobalDef {
                linkage: Linkage::Private,
                thread_local: false,
                constant: true,
                unnamed_addr: true,
                ty: Ty::bytes(3),
                init: Some(fmt::c_string(b"hi\0")),
                align: 1,
            },
        );
        let out = m.finish();
        assert!(out.contains("declare i32 @puts(ptr noundef) nounwind\n"));
        assert!(!out.contains("declare void @f"));
        assert!(out.contains("@s = private unnamed_addr constant [3 x i8] c\"hi\\00\", align 1\n"));
    }
}
