//! Module-wide LLVM lowering state: the shared codegen context, the module under construction, and
//! the signature of every function a call can name (generated here, a host import, or the runtime).

use super::ir::{
    CallConv, Decl, FnAttr, FnTy, FunctionWriter, GlobalDef, Linkage, MdRef, ModuleWriter,
    ParamAttr, Repr, Ty, Value,
};
use super::runtime_sigs::RuntimeSigs;
use crate::backend::shared::Target;
use crate::backend::shared::cx::Cx;
use crate::{Mir, MirFunction};
use dream_types::TypeInterner;
use indexmap::{IndexMap, IndexSet};
use std::sync::Arc;

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
    pub cx: Arc<Cx<'a>>,
    pub mir: &'a Mir,
    pub requirements: crate::backend::RuntimeRequirements,
    pub interner: &'a TypeInterner,
    pub sigs: &'a RuntimeSigs,
    pub m: ModuleWriter,
    own: IndexMap<String, FnSig>,
    /// Internal functions returning a value struct through a trailing caller buffer instead of a
    /// heap box. Tables and itables point at their `__abi` wrapper, which keeps the box ABI.
    pub sret: IndexSet<String>,
    /// Indirect callers receive an owned result from a borrowed-result getter.
    pub borrowed_results: IndexSet<String>,
    pub type_infos: IndexMap<i32, String>,
    pub next_info_index: i32,
    pub future_infos: IndexSet<dream_types::TypeId>,
    pub future_copies: IndexSet<dream_types::TypeId>,
    /// Types whose generated structural `==` body has been requested (`glue/equality.rs`).
    pub value_eq: IndexSet<dream_types::TypeId>,
    /// Library functions taking the caller's panic location as a trailing hidden `ptr`
    /// (`source_loc.rs`). Tables and itables point at their `__abi` wrapper, which passes NULL.
    pub tracked: IndexSet<String>,
    hosts: IndexMap<String, FnSig>,
    intrinsics: IndexMap<String, FnTy>,
    /// wasm32 export names of functions this module defines.
    exports: IndexMap<String, String>,
    /// Functions the C shim calls, so they must stay visible until the shim is linked in.
    shim_callees: IndexSet<String>,
    /// NUL-terminated constant strings by contents, each a private global.
    byte_blobs: IndexMap<Vec<u8>, String>,
    cstrs: IndexMap<String, String>,
    pub dbg_cu: Option<MdRef>,
    /// Debugger view composites by name (`debug_views.rs`).
    pub dbg_views: IndexMap<String, MdRef>,
    pub std_sources: Option<std::path::PathBuf>,
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
        let next_info_index = cx
            .tags
            .values()
            .copied()
            .max()
            .map_or(crate::abi::TAG_STRUCT_BASE, |tag| tag + 1);
        Self {
            cx: Arc::new(cx),
            mir,
            requirements: crate::backend::RuntimeRequirements::from_mir(mir, interner),
            interner,
            sigs,
            m,
            own: IndexMap::new(),
            sret: IndexSet::new(),
            borrowed_results: IndexSet::new(),
            type_infos: IndexMap::new(),
            next_info_index,
            future_infos: IndexSet::new(),
            future_copies: IndexSet::new(),
            tracked: IndexSet::new(),
            value_eq: IndexSet::new(),
            hosts: IndexMap::new(),
            intrinsics: IndexMap::new(),
            exports: IndexMap::new(),
            shim_callees: IndexSet::new(),
            cstrs: IndexMap::new(),
            byte_blobs: IndexMap::new(),
            dbg_cu: None,
            dbg_views: IndexMap::new(),
            std_sources: None,
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

    /// Registers a function this module defines that the generated C shim calls.
    pub fn own_shim_callee(&mut self, name: &str, sig: FnSig) {
        self.shim_callees.insert(name.to_string());
        self.own(name, sig);
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

    /// Marks a public definition, with its JS export name on wasm32.
    pub fn export(&mut self, name: &str, export: &str) {
        if self
            .cx
            .target
            .spec()
            .execution_environment
            .component()
            .is_some()
        {
            return;
        }
        self.exports.insert(name.to_string(), export.to_string());
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
        w.linkage = if name == "main"
            || (name == "__main_void"
                && self
                    .cx
                    .target
                    .spec()
                    .execution_environment
                    .component()
                    .is_some())
            || export.is_some()
            || self.shim_callees.contains(name)
            || self.sigs.has_function(name)
        {
            Linkage::External
        } else {
            Linkage::Internal
        };
        for (k, v) in &self.sigs.target_attrs {
            w.attrs.push(FnAttr::Str(k.clone(), v.clone()));
        }
        if let Some(e) = export.filter(|_| self.cx.target.spec().capabilities.js_interop) {
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
                Ty::F16 => 2,
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
        let offset = crate::abi::HEAP_HEADER_SIZE;
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

    /// `ptr` to a private NUL-terminated copy of `s`, shared by every use of the same contents.
    pub fn cstr(&mut self, s: &str) -> Value {
        if let Some(name) = self.cstrs.get(s) {
            return Value::global(name.clone());
        }
        let name = constant_name(".cstr", s.as_bytes());
        let bytes: Vec<u8> = s.bytes().chain(std::iter::once(0)).collect();
        self.m.global(
            &name,
            GlobalDef {
                linkage: Linkage::Private,
                thread_local: false,
                constant: true,
                unnamed_addr: true,
                ty: Ty::bytes(bytes.len() as u64),
                init: Some(super::ir::fmt::c_string(&bytes)),
                align: 1,
            },
        );
        self.cstrs.insert(s.to_string(), name.clone());
        Value::global(name)
    }

    /// Shares immutable byte payloads across array-literal uses in deterministic emission order.
    pub fn byte_blob(&mut self, bytes: &[u8]) -> Value {
        if let Some(name) = self.byte_blobs.get(bytes) {
            return Value::global(name.clone());
        }
        let name = constant_name(".bytes", bytes);
        self.m.global(
            &name,
            GlobalDef {
                linkage: Linkage::Private,
                thread_local: false,
                constant: true,
                unnamed_addr: true,
                ty: Ty::bytes(bytes.len() as u64),
                init: Some(super::ir::fmt::c_string(bytes)),
                align: 1,
            },
        );
        self.byte_blobs.insert(bytes.to_vec(), name.clone());
        Value::global(name)
    }

    pub fn global(&mut self, name: &str, def: GlobalDef) {
        self.m.global(name, def);
    }

    /// The symbol an indirect call (function table, itable, guarded interface arm) may use: the
    /// plain-ABI wrapper for a buffer-returning or caller-tracking function, the function itself
    /// otherwise.
    pub fn abi_sym(&self, name: &str) -> String {
        if self.has_abi_wrapper(name) {
            format!("{name}__abi")
        } else {
            name.to_string()
        }
    }

    pub fn has_abi_wrapper(&self, name: &str) -> bool {
        self.sret.contains(name)
            || self.tracked.contains(name)
            || self.borrowed_results.contains(name)
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

    /// A copy that builds some function bodies on another thread; `merge` folds it back.
    pub fn fork(&self) -> Lcx<'a> {
        Lcx {
            cx: Arc::clone(&self.cx),
            mir: self.mir,
            requirements: self.requirements,
            interner: self.interner,
            sigs: self.sigs,
            m: self.m.fork(),
            own: self.own.clone(),
            sret: self.sret.clone(),
            borrowed_results: self.borrowed_results.clone(),
            type_infos: self.type_infos.clone(),
            next_info_index: self.next_info_index,
            future_infos: self.future_infos.clone(),
            future_copies: self.future_copies.clone(),
            value_eq: self.value_eq.clone(),
            tracked: self.tracked.clone(),
            hosts: self.hosts.clone(),
            intrinsics: self.intrinsics.clone(),
            exports: self.exports.clone(),
            shim_callees: self.shim_callees.clone(),
            byte_blobs: self.byte_blobs.clone(),
            cstrs: self.cstrs.clone(),
            dbg_cu: self.dbg_cu,
            dbg_views: self.dbg_views.clone(),
            std_sources: self.std_sources.clone(),
        }
    }

    pub fn sizes(&self) -> LcxSizes {
        LcxSizes {
            m: self.m.sizes(),
            own: self.own.len(),
            sret: self.sret.len(),
            borrowed_results: self.borrowed_results.len(),
            type_infos: self.type_infos.len(),
            next_info_index: self.next_info_index,
            future_infos: self.future_infos.len(),
            future_copies: self.future_copies.len(),
            value_eq: self.value_eq.len(),
            tracked: self.tracked.len(),
            hosts: self.hosts.len(),
            intrinsics: self.intrinsics.len(),
            exports: self.exports.len(),
            shim_callees: self.shim_callees.len(),
            byte_blobs: self.byte_blobs.len(),
            cstrs: self.cstrs.len(),
            dbg_views: self.dbg_views.len(),
        }
    }

    /// Whether `fork`, forked at `base`, touched state numbered in build order (type metadata
    /// tags, future glue, debug metadata). Such a fork cannot be merged: its numbers would
    /// depend on which functions other threads built first.
    pub fn order_dependent(&self, fork: &Lcx<'_>, base: &LcxSizes) -> bool {
        fork.type_infos.len() != base.type_infos
            || fork.next_info_index != base.next_info_index
            || fork.future_infos.len() != base.future_infos
            || fork.future_copies.len() != base.future_copies
            || fork.dbg_views.len() != base.dbg_views
            || fork.dbg_cu != self.dbg_cu
            || fork.m.md.node_count() != 0
            || fork.sret.len() != base.sret
            || fork.borrowed_results.len() != base.borrowed_results
            || fork.tracked.len() != base.tracked
    }

    /// Appends what `fork` added since `base`, in the order it added it, as if its functions had
    /// been built here.
    pub fn merge(&mut self, fork: Lcx<'_>, base: &LcxSizes) {
        fn extend<K: std::hash::Hash + Eq, V>(
            into: &mut IndexMap<K, V>,
            from: IndexMap<K, V>,
            skip: usize,
        ) {
            for (key, value) in from.into_iter().skip(skip) {
                into.entry(key).or_insert(value);
            }
        }
        fn extend_set<K: std::hash::Hash + Eq>(into: &mut IndexSet<K>, from: IndexSet<K>, skip: usize) {
            into.extend(from.into_iter().skip(skip));
        }
        self.m.merge(fork.m, base.m);
        extend(&mut self.own, fork.own, base.own);
        extend_set(&mut self.value_eq, fork.value_eq, base.value_eq);
        extend(&mut self.hosts, fork.hosts, base.hosts);
        extend(&mut self.intrinsics, fork.intrinsics, base.intrinsics);
        extend(&mut self.exports, fork.exports, base.exports);
        extend_set(&mut self.shim_callees, fork.shim_callees, base.shim_callees);
        extend(&mut self.byte_blobs, fork.byte_blobs, base.byte_blobs);
        extend(&mut self.cstrs, fork.cstrs, base.cstrs);
    }
}

/// Named by contents, so a constant's name does not depend on which function used it first.
fn constant_name(prefix: &str, bytes: &[u8]) -> String {
    let digest = blake3::hash(bytes).to_hex();
    format!("{prefix}.{}", &digest[..32])
}

/// Table sizes of an `Lcx` at the moment it was forked.
pub(super) struct LcxSizes {
    m: super::ir::ModuleSizes,
    own: usize,
    sret: usize,
    borrowed_results: usize,
    type_infos: usize,
    next_info_index: i32,
    future_infos: usize,
    future_copies: usize,
    value_eq: usize,
    tracked: usize,
    hosts: usize,
    intrinsics: usize,
    exports: usize,
    shim_callees: usize,
    byte_blobs: usize,
    cstrs: usize,
    dbg_views: usize,
}
