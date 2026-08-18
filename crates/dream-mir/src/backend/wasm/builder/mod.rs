//! Named WASM module/function builders on top of `wasm-encoder`.
//!
//! Instruction streams are structured (`Op`), never concatenated WAT text. Hand-written
//! `runtime/*.wat` is parsed with `wast` and lowered into the same builders by name.

mod encode;
mod func;
mod inst;
mod wast_in;

pub(crate) use func::{BlockTy, ExtractLane, FuncBuilder, Label, LoadKind, ReplaceLane, StoreKind};
pub(crate) use inst::Nullary;
pub(crate) use wasm_encoder::ValType;

use crate::internal_error;
use indexmap::{IndexMap, IndexSet};
use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, ElementSection, Elements, ExportKind, ExportSection,
    FunctionSection, GlobalSection, GlobalType, ImportSection, IndirectNameMap, MemoryType, Module,
    NameMap, NameSection, RefType, StartSection, TableSection, TableType, TypeSection,
};

#[derive(Clone, Copy)]
enum ExportItem {
    Func,
    Table,
    Memory,
    Global,
}

struct FuncImport {
    module: String,
    field: String,
    name: String,
    params: Vec<ValType>,
    results: Vec<ValType>,
}

struct MemoryImport {
    module: String,
    field: String,
    min: u32,
    max: u32,
}

struct TableDef {
    name: String,
    min: u32,
    max: u32,
}

struct GlobalDef {
    ty: ValType,
    mutable: bool,
    init: i64,
    is_f32: bool,
    is_f64: bool,
}

struct ElemDef {
    table: String,
    offset: i32,
    funcs: Vec<String>,
}

struct DataDef {
    offset: u32,
    bytes: Vec<u8>,
}

/// Whole-module encoder: named functions/globals/types, binary `finish`, optional DCE.
pub(crate) struct ModuleBuilder {
    type_list: Vec<(Vec<ValType>, Vec<ValType>)>,
    type_sigs: IndexMap<(Vec<ValType>, Vec<ValType>), u32>,
    type_names: IndexMap<String, u32>,
    func_imports: Vec<FuncImport>,
    memory: Option<MemoryImport>,
    tables: Vec<TableDef>,
    globals: IndexMap<String, GlobalDef>,
    funcs: IndexMap<String, FuncBuilder>,
    exports: Vec<(String, ExportItem, String)>,
    start: Option<String>,
    elems: Vec<ElemDef>,
    data: Vec<DataDef>,
    /// When true, drop unreachable funcs and unused func imports before encoding.
    pub dce: bool,
    anon: u32,
}

impl ModuleBuilder {
    pub(crate) fn new() -> Self {
        Self {
            type_list: Vec::new(),
            type_sigs: IndexMap::new(),
            type_names: IndexMap::new(),
            func_imports: Vec::new(),
            memory: None,
            tables: Vec::new(),
            globals: IndexMap::new(),
            funcs: IndexMap::new(),
            exports: Vec::new(),
            start: None,
            elems: Vec::new(),
            data: Vec::new(),
            dce: false,
            anon: 0,
        }
    }

    pub(crate) fn intern_type(
        &mut self,
        name: Option<&str>,
        params: Vec<ValType>,
        results: Vec<ValType>,
    ) -> u32 {
        let name = name.map(|n| strip_dollar(n).to_string());
        if let Some(n) = name.as_deref() {
            if let Some(&i) = self.type_names.get(n) {
                return i;
            }
        }
        let key = (params.clone(), results.clone());
        if let Some(&i) = self.type_sigs.get(&key) {
            if let Some(n) = name {
                self.type_names.insert(n, i);
            }
            return i;
        }
        let i = self.type_list.len() as u32;
        self.type_list.push(key.clone());
        self.type_sigs.insert(key, i);
        if let Some(n) = name {
            self.type_names.insert(n, i);
        }
        i
    }

    pub(crate) fn has_func(&self, name: &str) -> bool {
        let n = strip_dollar(name);
        self.funcs.contains_key(n)
            || self
                .func_imports
                .iter()
                .any(|i| i.name == n || i.field == n)
    }

    pub(crate) fn import_func(
        &mut self,
        module: &str,
        field: &str,
        name: &str,
        params: Vec<ValType>,
        results: Vec<ValType>,
    ) {
        if self.func_imports.iter().any(|i| i.name == name) {
            return;
        }
        self.intern_type(None, params.clone(), results.clone());
        self.func_imports.push(FuncImport {
            module: module.to_string(),
            field: field.to_string(),
            name: strip_dollar(name).to_string(),
            params,
            results,
        });
    }

    pub(crate) fn import_memory(&mut self, module: &str, field: &str, min: u32, max: u32) {
        self.memory = Some(MemoryImport {
            module: module.to_string(),
            field: field.to_string(),
            min,
            max,
        });
    }

    pub(crate) fn table(&mut self, name: &str, min: u32, max: u32) {
        self.tables.push(TableDef {
            name: strip_dollar(name).to_string(),
            min,
            max,
        });
    }

    pub(crate) fn global_i32(&mut self, name: &str, mutable: bool, init: i32) {
        self.globals.insert(
            strip_dollar(name).to_string(),
            GlobalDef {
                ty: ValType::I32,
                mutable,
                init: init as i64,
                is_f32: false,
                is_f64: false,
            },
        );
    }

    pub(crate) fn global(
        &mut self,
        name: &str,
        ty: ValType,
        mutable: bool,
        init: i64,
        is_f32: bool,
        is_f64: bool,
    ) {
        self.globals.insert(
            strip_dollar(name).to_string(),
            GlobalDef {
                ty,
                mutable,
                init,
                is_f32,
                is_f64,
            },
        );
    }

    pub(crate) fn push_func(&mut self, f: FuncBuilder) {
        self.funcs.insert(f.name.clone(), f);
    }

    #[allow(dead_code)]
    pub(crate) fn start_func(&self, name: &str) -> FuncBuilder {
        FuncBuilder::new(strip_dollar(name))
    }

    pub(crate) fn export_func(&mut self, export_name: &str, func: &str) {
        self.exports.push((
            export_name.to_string(),
            ExportItem::Func,
            strip_dollar(func).to_string(),
        ));
    }

    pub(crate) fn export_table(&mut self, export_name: &str, table: &str) {
        self.exports.push((
            export_name.to_string(),
            ExportItem::Table,
            strip_dollar(table).to_string(),
        ));
    }

    pub(crate) fn export_memory(&mut self, export_name: &str) {
        self.exports
            .push((export_name.to_string(), ExportItem::Memory, String::new()));
    }

    pub(crate) fn export_global(&mut self, export_name: &str, global: &str) {
        self.exports.push((
            export_name.to_string(),
            ExportItem::Global,
            strip_dollar(global).to_string(),
        ));
    }

    pub(crate) fn set_start(&mut self, name: &str) {
        self.start = Some(strip_dollar(name).to_string());
    }

    pub(crate) fn elem(&mut self, table: &str, offset: i32, funcs: Vec<String>) {
        self.elems.push(ElemDef {
            table: strip_dollar(table).to_string(),
            offset,
            funcs: funcs
                .into_iter()
                .map(|s| strip_dollar(&s).to_string())
                .collect(),
        });
    }

    pub(crate) fn data(&mut self, offset: u32, bytes: Vec<u8>) {
        self.data.push(DataDef { offset, bytes });
    }

    pub(crate) fn next_anon(&mut self) -> String {
        let n = self.anon;
        self.anon += 1;
        format!("__anon{n}")
    }

    pub(crate) fn ingest_wat(&mut self, fields: &str) {
        wast_in::ingest(self, fields);
    }

    /// Encode the module. When [`Self::dce`] is set, drop unreachable functions first.
    pub(crate) fn finish(mut self) -> Vec<u8> {
        if self.dce {
            self.strip_dead();
        }
        self.encode()
    }

    pub(crate) fn finish_wat(self) -> String {
        print_wasm(&self.finish())
    }

    fn func_index_map(&self) -> IndexMap<String, u32> {
        let mut map = IndexMap::new();
        let mut i = 0u32;
        for imp in &self.func_imports {
            map.insert(imp.name.clone(), i);
            i += 1;
        }
        for name in self.funcs.keys() {
            map.insert(name.clone(), i);
            i += 1;
        }
        map
    }

    fn global_index_map(&self) -> IndexMap<String, u32> {
        self.globals
            .keys()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as u32))
            .collect()
    }

    fn table_index_map(&self) -> IndexMap<String, u32> {
        self.tables
            .iter()
            .enumerate()
            .map(|(i, t)| (t.name.clone(), i as u32))
            .collect()
    }

    fn strip_dead(&mut self) {
        let mut live: IndexSet<String> = IndexSet::new();
        let mut q: Vec<String> = Vec::new();
        for (export, kind, item) in &self.exports {
            let _ = export;
            if matches!(kind, ExportItem::Func) {
                q.push(item.clone());
            }
        }
        if let Some(s) = &self.start {
            q.push(s.clone());
        }
        for e in &self.elems {
            q.extend(e.funcs.iter().cloned());
        }
        for name in q {
            live.insert(name);
        }
        let mut work: Vec<String> = live.iter().cloned().collect();
        while let Some(name) = work.pop() {
            if let Some(f) = self.funcs.get(&name) {
                for cal in f.callees() {
                    if live.insert(cal.clone()) {
                        work.push(cal);
                    }
                }
            }
        }
        self.funcs.retain(|n, _| live.contains(n));
        self.func_imports.retain(|i| live.contains(&i.name));
    }

    fn encode(mut self) -> Vec<u8> {
        let func_sigs: Vec<(Vec<ValType>, Vec<ValType>)> = self
            .funcs
            .values()
            .map(|f| (f.param_types(), f.results.clone()))
            .collect();
        for (p, r) in func_sigs {
            self.intern_type(None, p, r);
        }
        let import_sigs: Vec<(Vec<ValType>, Vec<ValType>)> = self
            .func_imports
            .iter()
            .map(|i| (i.params.clone(), i.results.clone()))
            .collect();
        for (p, r) in import_sigs {
            self.intern_type(None, p, r);
        }

        let func_idx = self.func_index_map();
        let global_idx = self.global_index_map();
        let table_idx = self.table_index_map();

        let mut module = Module::new();

        let mut types = TypeSection::new();
        for (params, results) in &self.type_list {
            types.ty().function(params.clone(), results.clone());
        }
        if !self.type_list.is_empty() {
            module.section(&types);
        }

        let mut imports = ImportSection::new();
        if let Some(mem) = &self.memory {
            imports.import(
                &mem.module,
                &mem.field,
                wasm_encoder::EntityType::Memory(MemoryType {
                    minimum: mem.min as u64,
                    maximum: Some(mem.max as u64),
                    memory64: false,
                    shared: true,
                    page_size_log2: None,
                }),
            );
        }
        let import_tys: Vec<u32> = self
            .func_imports
            .iter()
            .map(|imp| {
                *self
                    .type_sigs
                    .get(&(imp.params.clone(), imp.results.clone()))
                    .expect("import type interned")
            })
            .collect();
        for (imp, ty) in self.func_imports.iter().zip(import_tys) {
            imports.import(
                &imp.module,
                &imp.field,
                wasm_encoder::EntityType::Function(ty),
            );
        }
        if self.memory.is_some() || !self.func_imports.is_empty() {
            module.section(&imports);
        }

        let mut functions = FunctionSection::new();
        let func_tys: Vec<u32> = self
            .funcs
            .values()
            .map(|f| {
                *self
                    .type_sigs
                    .get(&(f.param_types(), f.results.clone()))
                    .expect("func type interned")
            })
            .collect();
        for ty in func_tys {
            functions.function(ty);
        }
        if !self.funcs.is_empty() {
            module.section(&functions);
        }

        if !self.tables.is_empty() {
            let mut tables = TableSection::new();
            for t in &self.tables {
                tables.table(TableType {
                    element_type: RefType::FUNCREF,
                    minimum: t.min as u64,
                    maximum: Some(t.max as u64),
                    table64: false,
                    shared: false,
                });
            }
            module.section(&tables);
        }

        if !self.globals.is_empty() {
            let mut globals = GlobalSection::new();
            for g in self.globals.values() {
                let init = if g.is_f32 {
                    ConstExpr::f32_const(wasm_encoder::Ieee32::from(f32::from_bits(g.init as u32)))
                } else if g.is_f64 {
                    ConstExpr::f64_const(wasm_encoder::Ieee64::from(f64::from_bits(g.init as u64)))
                } else if g.ty == ValType::I64 {
                    ConstExpr::i64_const(g.init)
                } else {
                    ConstExpr::i32_const(g.init as i32)
                };
                globals.global(
                    GlobalType {
                        val_type: g.ty,
                        mutable: g.mutable,
                        shared: false,
                    },
                    &init,
                );
            }
            module.section(&globals);
        }

        let mut exports = ExportSection::new();
        let mut any_export = false;
        for (name, kind, item) in &self.exports {
            match kind {
                ExportItem::Func => {
                    let Some(&idx) = func_idx.get(item) else {
                        continue;
                    };
                    exports.export(name, ExportKind::Func, idx);
                    any_export = true;
                }
                ExportItem::Table => {
                    let Some(&idx) = table_idx.get(item) else {
                        continue;
                    };
                    exports.export(name, ExportKind::Table, idx);
                    any_export = true;
                }
                ExportItem::Memory => {
                    exports.export(name, ExportKind::Memory, 0);
                    any_export = true;
                }
                ExportItem::Global => {
                    let Some(&idx) = global_idx.get(item) else {
                        continue;
                    };
                    exports.export(name, ExportKind::Global, idx);
                    any_export = true;
                }
            }
        }
        if any_export {
            module.section(&exports);
        }

        if let Some(s) = &self.start {
            let Some(&idx) = func_idx.get(s) else {
                internal_error!("start function ${s} was not defined");
            };
            module.section(&StartSection {
                function_index: idx,
            });
        }

        if !self.elems.is_empty() {
            let mut elems = ElementSection::new();
            for e in &self.elems {
                let table = *table_idx.get(&e.table).unwrap_or(&0);
                let idxs: Vec<u32> = e
                    .funcs
                    .iter()
                    .map(|n| {
                        *func_idx.get(n).unwrap_or_else(|| {
                            internal_error!("elem function ${n} was not defined")
                        })
                    })
                    .collect();
                elems.active(
                    Some(table),
                    &ConstExpr::i32_const(e.offset),
                    Elements::Functions(std::borrow::Cow::Owned(idxs)),
                );
            }
            module.section(&elems);
        }

        let mut code = CodeSection::new();
        for f in self.funcs.values() {
            code.function(&f.encode(&func_idx, &global_idx, &table_idx, &self.type_names));
        }
        if !self.funcs.is_empty() {
            module.section(&code);
        }

        if !self.data.is_empty() {
            let mut data = DataSection::new();
            for d in &self.data {
                data.active(
                    0,
                    &ConstExpr::i32_const(d.offset as i32),
                    d.bytes.iter().copied(),
                );
            }
            module.section(&data);
        }

        let mut names = NameSection::new();
        let mut fnames = NameMap::new();
        for (name, idx) in &func_idx {
            fnames.append(*idx, name);
        }
        names.functions(&fnames);

        let mut locals = IndirectNameMap::new();
        for (name, f) in &self.funcs {
            let Some(&idx) = func_idx.get(name) else {
                continue;
            };
            let mut map = NameMap::new();
            for (i, (ln, _)) in f.all_locals().iter().enumerate() {
                if !ln.is_empty() {
                    map.append(i as u32, ln);
                }
            }
            if !map.is_empty() {
                locals.append(idx, &map);
            }
        }
        names.locals(&locals);

        let mut tnames = NameMap::new();
        {
            let mut by_idx: std::collections::BTreeMap<u32, String> =
                std::collections::BTreeMap::new();
            for (name, idx) in &self.type_names {
                by_idx.entry(*idx).or_insert_with(|| name.clone());
            }
            for (idx, name) in by_idx {
                tnames.append(idx, &name);
            }
        }
        let mut tabnames = NameMap::new();
        let mut tabs: Vec<(u32, String)> = table_idx.iter().map(|(n, i)| (*i, n.clone())).collect();
        tabs.sort_by_key(|(i, _)| *i);
        for (idx, name) in tabs {
            tabnames.append(idx, &name);
        }
        let mut gnames = NameMap::new();
        let mut globs: Vec<(u32, String)> =
            global_idx.iter().map(|(n, i)| (*i, n.clone())).collect();
        globs.sort_by_key(|(i, _)| *i);
        for (idx, name) in globs {
            gnames.append(idx, &name);
        }
        if !tnames.is_empty() {
            names.types(&tnames);
        }
        if !tabnames.is_empty() {
            names.tables(&tabnames);
        }
        if !gnames.is_empty() {
            names.globals(&gnames);
        }

        module.section(&names);
        module.finish()
    }
}

pub fn print_wasm(bytes: &[u8]) -> String {
    wasmprinter::print_bytes(bytes)
        .unwrap_or_else(|e| internal_error!("wasmprinter failed on encoded module: {e}"))
}

pub(crate) fn strip_dollar(s: &str) -> &str {
    s.strip_prefix('$').unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_add() {
        let mut m = ModuleBuilder::new();
        let mut f = m.start_func("add");
        f.param("a", ValType::I32);
        f.param("b", ValType::I32);
        f.result(ValType::I32);
        f.local_get("a");
        f.local_get("b");
        f.i32_add();
        f.return_();
        m.push_func(f);
        m.export_func("add", "add");
        let bytes = m.finish();
        let wat = print_wasm(&bytes);
        assert!(wat.contains("i32.add"), "{wat}", wat = wat);
        assert!(wat.contains("$add"), "{wat}", wat = wat);
    }

    #[test]
    fn dce_drops_unexported() {
        let mut m = ModuleBuilder::new();
        m.dce = true;
        let mut live = m.start_func("live");
        live.call("helper");
        m.push_func(live);
        let mut helper = m.start_func("helper");
        helper.i32_const(0);
        helper.drop_();
        m.push_func(helper);
        let mut dead = m.start_func("dead");
        dead.i32_const(1);
        dead.drop_();
        m.push_func(dead);
        m.export_func("live", "live");
        let wat = m.finish_wat();
        assert!(wat.contains("$live"), "{wat}", wat = wat);
        assert!(wat.contains("$helper"), "{wat}", wat = wat);
        assert!(!wat.contains("$dead"), "{wat}", wat = wat);
    }
}
