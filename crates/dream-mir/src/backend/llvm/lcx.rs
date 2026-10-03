//! Module-wide LLVM lowering state: the shared codegen context, the module under construction, and
//! the signature of every function a call can name (generated here, a host import, or the runtime).

use super::ir::{
    CallConv, Decl, FnAttr, FnTy, FunctionWriter, GlobalDef, Linkage, MdRef, ModuleWriter,
    ParamAttr, Repr, Ty, Value,
};
use super::runtime_sigs::RuntimeSigs;
use crate::backend::shared::cx::Cx;
use crate::backend::shared::Target;
use crate::{Mir, MirFunction};
use dream_types::TypeInterner;
use indexmap::{IndexMap, IndexSet};

#[derive(Clone, Debug)]
pub(super) struct FnSig {
    pub fty: FnTy,
    pub ret_attrs: Vec<ParamAttr>,
    pub param_attrs: Vec<Vec<ParamAttr>>,
    pub noreturn: bool,
    /// wasm32 `(module, field)` a declaration imports from the host.
    pub import: Option<(String, String)>,
}

impl FnSig {
    pub fn plain(fty: FnTy) -> Self {
        Self {
            fty,
            ret_attrs: Vec::new(),
            param_attrs: Vec::new(),
            noreturn: false,
            import: None,
        }
    }

    fn of_runtime(rt: &super::runtime_sigs::RtFn) -> Self {
        Self {
            fty: rt.fty.clone(),
            ret_attrs: rt.ret_attrs.clone(),
            param_attrs: rt.param_attrs.clone(),
            noreturn: rt.noreturn,
            import: rt.wasm_import.clone(),
        }
    }

    pub fn ret_unsigned(&self) -> bool {
        self.ret_attrs.contains(&ParamAttr::ZeroExt)
    }
}

pub(super) struct Lcx<'a> {
    pub cx: Cx<'a>,
    pub mir: &'a Mir,
    pub interner: &'a TypeInterner,
    pub sigs: &'a RuntimeSigs,
    pub m: ModuleWriter,
    own: IndexMap<String, FnSig>,
    /// Internal functions returning a value struct through a trailing caller buffer instead of a
    /// heap box. Tables and itables point at their `__boxed` wrapper, which keeps the box ABI.
    pub sret: IndexSet<String>,
    hosts: IndexMap<String, FnSig>,
    intrinsics: IndexMap<String, FnTy>,
    /// wasm32 export names of functions this module defines.
    exports: IndexMap<String, String>,
    pub dbg_cu: Option<MdRef>,
    /// Debugger view composites by name (`debug_views.rs`).
    pub dbg_views: IndexMap<String, MdRef>,
}

impl<'a> Lcx<'a> {
    pub fn new(
        mir: &'a Mir,
        interner: &'a TypeInterner,
        sigs: &'a RuntimeSigs,
        leak: bool,
        target: Target,
    ) -> Self {
        let cx = Cx::with_leak_checks(mir, interner, target, leak);
        let m = ModuleWriter::new("dream", &sigs.triple, &sigs.datalayout);
        Self {
            cx,
            mir,
            interner,
            sigs,
            m,
            own: IndexMap::new(),
            sret: IndexSet::new(),
            hosts: IndexMap::new(),
            intrinsics: IndexMap::new(),
            exports: IndexMap::new(),
            dbg_cu: None,
            dbg_views: IndexMap::new(),
        }
    }

    /// Registers a function this module defines. A name the runtime also knows (a weak default or
    /// a hook it calls back) takes the runtime's exact type so the two modules link.
    pub fn own(&mut self, name: &str, sig: FnSig) {
        let sig = match self.sigs.fns.get(name) {
            Some(rt) => FnSig::of_runtime(rt),
            None => sig,
        };
        self.own.insert(name.to_string(), sig);
    }

    /// A host import the runtime header does not declare; its C type follows the Dream signature.
    pub fn host(&mut self, name: &str, sig: FnSig) {
        if !self.sigs.has_function(name) {
            self.hosts.insert(name.to_string(), sig);
        }
    }

    /// A wasm32 host import `(module, field)`. A runtime-declared name keeps the runtime's type;
    /// the import names always come from the Dream extern.
    pub fn wasm_import(&mut self, name: &str, sig: FnSig, module: &str, field: &str) {
        let mut sig = match self.sigs.fns.get(name) {
            Some(rt) => FnSig::of_runtime(rt),
            None => sig,
        };
        sig.import = Some((module.to_string(), field.to_string()));
        self.hosts.insert(name.to_string(), sig);
    }

    /// Exports a function this module defines under `export` (wasm32 only).
    pub fn export(&mut self, name: &str, export: &str) {
        if self.cx.target.spec().capabilities.js_interop {
            self.exports.insert(name.to_string(), export.to_string());
        }
    }

    /// The signature of a callable symbol, declaring it when it lives outside this module.
    pub fn sig(&mut self, name: &str) -> FnSig {
        if let Some(s) = self.own.get(name) {
            return s.clone();
        }
        if let Some(s) = self.hosts.get(name).cloned() {
            self.declare(name, &s);
            return s;
        }
        let Some(rt) = self.sigs.fns.get(name) else {
            std::panic::panic_any(super::MissingRuntimeSymbol(name.to_string()));
        };
        let s = FnSig::of_runtime(rt);
        self.declare(name, &s);
        s
    }

    fn declare(&mut self, name: &str, s: &FnSig) {
        let mut attrs = vec![FnAttr::NoUnwind];
        if s.noreturn {
            attrs.push(FnAttr::NoReturn);
        }
        if let Some((module, field)) = &s.import {
            attrs.push(FnAttr::Str("wasm-import-module".into(), module.clone()));
            attrs.push(FnAttr::Str("wasm-import-name".into(), field.clone()));
        }
        self.m.declare(
            name,
            Decl {
                fty: s.fty.clone(),
                ret_attrs: s.ret_attrs.clone(),
                param_attrs: s.param_attrs.clone(),
                attrs,
                cc: CallConv::C,
            },
        );
    }

    /// `ptr @name` for a callable symbol (declared on demand).
    pub fn fn_ref(&mut self, name: &str) -> Value {
        let _ = self.sig(name);
        Value::global(name)
    }

    pub fn intrinsic(&mut self, name: &str, fty: FnTy) -> FnSig {
        if !self.intrinsics.contains_key(name) {
            self.intrinsics.insert(name.to_string(), fty.clone());
            self.m.declare(
                name,
                Decl {
                    fty: fty.clone(),
                    ret_attrs: vec![],
                    param_attrs: vec![],
                    attrs: vec![FnAttr::NoUnwind],
                    cc: CallConv::C,
                },
            );
        }
        FnSig::plain(self.intrinsics[name].clone())
    }

    /// A fresh writer for a registered definition, carrying the runtime's target attributes.
    pub fn writer(&self, name: &str) -> FunctionWriter {
        let sig = self.own.get(name).unwrap_or_else(|| {
            crate::internal_error!("LLVM function `{name}` was never registered")
        });
        let params = sig
            .fty
            .params
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    t.clone(),
                    sig.param_attrs.get(i).cloned().unwrap_or_default(),
                )
            })
            .collect();
        let mut w = FunctionWriter::new(name, sig.fty.ret.clone(), params);
        w.ret_attrs = sig.ret_attrs.clone();
        let export = self.exports.get(name);
        w.linkage = if name == "main" || export.is_some() || self.sigs.has_function(name) {
            Linkage::External
        } else {
            Linkage::Internal
        };
        for (k, v) in &self.sigs.target_attrs {
            w.attrs.push(FnAttr::Str(k.clone(), v.clone()));
        }
        if let Some(e) = export {
            w.attrs
                .push(FnAttr::Str("wasm-export-name".into(), e.clone()));
        }
        w
    }

    /// `ptr @name` of a global the runtime defines, declared external with the runtime's type.
    pub fn rt_global(&mut self, name: &str) -> (Value, Ty) {
        let Some(g) = self.sigs.globals.get(name) else {
            std::panic::panic_any(super::MissingRuntimeSymbol(name.to_string()));
        };
        let ty = g.ty.clone();
        if !self.m.has_global(name) {
            let align = match &ty {
                Ty::Int(b) => (b / 8).clamp(1, 8),
                Ty::F32 => 4,
                _ => 8,
            };
            self.m.global(
                name,
                GlobalDef {
                    linkage: Linkage::External,
                    thread_local: g.thread_local,
                    constant: false,
                    unnamed_addr: false,
                    ty: ty.clone(),
                    init: None,
                    align,
                },
            );
        }
        (Value::global(name), ty)
    }

    pub fn define(&mut self, w: FunctionWriter) {
        self.m.define(w);
    }

    /// Immortal literal payload address; native references preserve LLVM pointer provenance.
    pub fn str_val(&self, s: &str) -> Value {
        let h = self.h();
        let symbol = self.cx.str_sym(s);
        let block = super::ir::fmt::global(&format!("{symbol}_blk"));
        let index = self.word();
        let offset = self.cx.target.abi().heap_header_size;
        let address = format!("getelementptr (i8, ptr {block}, {index} {offset})");
        Value::new(
            h.clone(),
            Repr::ConstExpr(if h == Ty::Ptr {
                address
            } else {
                format!("ptrtoint (ptr {address} to {h})")
            }),
        )
    }

    pub fn global(&mut self, name: &str, def: GlobalDef) {
        self.m.global(name, def);
    }

    /// The symbol an indirect call (function table, itable, guarded interface arm) may use: the
    /// box-ABI wrapper for a buffer-returning function, the function itself otherwise.
    pub fn boxed_sym(&self, name: &str) -> String {
        if self.sret.contains(name) {
            format!("{name}__boxed")
        } else {
            name.to_string()
        }
    }

    pub fn h(&self) -> Ty {
        if !self.cx.target.spec().capabilities.linear_memory {
            return Ty::Ptr;
        }
        self.word()
    }

    pub fn word(&self) -> Ty {
        Ty::Int(self.cx.target.abi().ptr_size * 8)
    }

    pub fn user_fn(&self, f: &MirFunction) -> String {
        crate::backend::shared::abi_types::c_ident(&crate::backend::shared::func_symbol(f))
    }
}
