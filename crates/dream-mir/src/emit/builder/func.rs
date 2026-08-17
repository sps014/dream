//! Named function body builder.

use super::inst::Nullary;
use super::{strip_dollar, ValType};
use indexmap::IndexMap;
use wasm_encoder::Function;

#[derive(Clone, Copy)]
pub(crate) enum BlockTy {
    Empty,
    I32,
    I64,
    F32,
    F64,
    V128,
}

#[derive(Clone)]
pub(crate) enum Label {
    Name(String),
    Depth(u32),
}

#[derive(Clone, Copy)]
pub(crate) enum LoadKind {
    I32,
    I32_8S,
    I32_8U,
    I32_16S,
    I32_16U,
    I64,
    I64_8U,
    I64_16U,
    I64_32U,
    F32,
    F64,
    V128,
}

#[derive(Clone, Copy)]
pub(crate) enum ExtractLane {
    I8x16S,
    I8x16U,
    I16x8S,
    I16x8U,
    I32x4,
    I64x2,
    F32x4,
    F64x2,
}

#[derive(Clone, Copy)]
pub(crate) enum ReplaceLane {
    I8x16,
    I16x8,
    I32x4,
    I64x2,
    F32x4,
    F64x2,
}

#[derive(Clone, Copy)]
pub(crate) enum StoreKind {
    I32,
    I32_8,
    I32_16,
    I64,
    I64_8,
    I64_16,
    I64_32,
    F32,
    F64,
    V128,
}

#[derive(Clone)]
pub(crate) enum Op {
    Unreachable,
    Nop,
    Block {
        label: String,
        ty: BlockTy,
    },
    Loop {
        label: String,
        ty: BlockTy,
    },
    If {
        label: String,
        ty: BlockTy,
    },
    Else,
    End,
    Br(Label),
    BrIf(Label),
    BrTable {
        labels: Vec<Label>,
        default: Label,
    },
    Return,
    Call(String),
    ReturnCall(String),
    CallIndirect {
        type_name: String,
        table: String,
    },
    Drop,
    Select,
    LocalGet(String),
    LocalSet(String),
    LocalTee(String),
    GlobalGet(String),
    GlobalSet(String),
    Load {
        kind: LoadKind,
        offset: u32,
        align: u32,
    },
    Store {
        kind: StoreKind,
        offset: u32,
        align: u32,
    },
    MemorySize,
    MemoryGrow,
    MemoryCopy,
    MemoryFill,
    I32Const(i32),
    I64Const(i64),
    F32Const(u32),
    F64Const(u64),
    Nullary(Nullary),
    AtomicLoad {
        offset: u32,
    },
    AtomicStore {
        offset: u32,
    },
    AtomicRmwAdd {
        offset: u32,
    },
    AtomicRmwSub {
        offset: u32,
    },
    AtomicRmwCmpxchg {
        offset: u32,
    },
    MemoryAtomicNotify {
        offset: u32,
    },
    MemoryAtomicWait32 {
        offset: u32,
    },
    RefFunc(String),
    ExtractLane {
        kind: ExtractLane,
        lane: u8,
    },
    ReplaceLane {
        kind: ReplaceLane,
        lane: u8,
    },
    V128Const([u8; 16]),
    Shuffle([u8; 16]),
}

pub(crate) struct FuncBuilder {
    pub(super) name: String,
    params: Vec<(String, ValType)>,
    pub(super) results: Vec<ValType>,
    locals: Vec<(String, ValType)>,
    ops: Vec<Op>,
}

#[allow(dead_code)]
impl FuncBuilder {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            name: strip_dollar(&name.into()).to_string(),
            params: Vec::new(),
            results: Vec::new(),
            locals: Vec::new(),
            ops: Vec::new(),
        }
    }

    pub(crate) fn param(&mut self, name: &str, ty: ValType) {
        self.params.push((strip_dollar(name).to_string(), ty));
    }

    pub(crate) fn result(&mut self, ty: ValType) {
        self.results.push(ty);
    }

    pub(crate) fn local(&mut self, name: &str, ty: ValType) {
        let n = strip_dollar(name).to_string();
        if self.params.iter().any(|(p, _)| p == &n) || self.locals.iter().any(|(p, _)| p == &n) {
            return;
        }
        self.locals.push((n, ty));
    }

    pub(crate) fn param_types(&self) -> Vec<ValType> {
        self.params.iter().map(|(_, t)| *t).collect()
    }

    pub(crate) fn all_locals(&self) -> Vec<(String, ValType)> {
        let mut v = self.params.clone();
        v.extend(self.locals.iter().cloned());
        v
    }

    pub(crate) fn checkpoint(&self) -> usize {
        self.ops.len()
    }

    pub(crate) fn rewind(&mut self, n: usize) {
        self.ops.truncate(n);
    }

    pub(crate) fn push(&mut self, op: Op) {
        self.ops.push(op);
    }

    pub(crate) fn nullary(&mut self, op: Nullary) {
        self.ops.push(Op::Nullary(op));
    }

    pub(crate) fn nop(&mut self) {
        self.ops.push(Op::Nop);
    }

    pub(crate) fn local_get(&mut self, name: &str) {
        self.ops.push(Op::LocalGet(strip_dollar(name).to_string()));
    }

    pub(crate) fn local_set(&mut self, name: &str) {
        self.ops.push(Op::LocalSet(strip_dollar(name).to_string()));
    }

    pub(crate) fn local_tee(&mut self, name: &str) {
        self.ops.push(Op::LocalTee(strip_dollar(name).to_string()));
    }

    pub(crate) fn global_get(&mut self, name: &str) {
        self.ops.push(Op::GlobalGet(strip_dollar(name).to_string()));
    }

    pub(crate) fn global_set(&mut self, name: &str) {
        self.ops.push(Op::GlobalSet(strip_dollar(name).to_string()));
    }

    pub(crate) fn call(&mut self, name: &str) {
        self.ops.push(Op::Call(strip_dollar(name).to_string()));
    }

    pub(crate) fn return_call(&mut self, name: &str) {
        self.ops
            .push(Op::ReturnCall(strip_dollar(name).to_string()));
    }

    pub(crate) fn call_indirect(&mut self, type_name: &str, table: &str) {
        self.ops.push(Op::CallIndirect {
            type_name: strip_dollar(type_name).to_string(),
            table: strip_dollar(table).to_string(),
        });
    }

    pub(crate) fn i32_const(&mut self, v: i32) {
        self.ops.push(Op::I32Const(v));
    }

    pub(crate) fn i64_const(&mut self, v: i64) {
        self.ops.push(Op::I64Const(v));
    }

    pub(crate) fn f32_const(&mut self, v: f32) {
        self.ops.push(Op::F32Const(v.to_bits()));
    }

    pub(crate) fn f64_const(&mut self, v: f64) {
        self.ops.push(Op::F64Const(v.to_bits()));
    }

    pub(crate) fn return_(&mut self) {
        self.ops.push(Op::Return);
    }

    pub(crate) fn drop_(&mut self) {
        self.ops.push(Op::Drop);
    }

    pub(crate) fn select(&mut self) {
        self.ops.push(Op::Select);
    }

    pub(crate) fn memory_copy(&mut self) {
        self.ops.push(Op::MemoryCopy);
    }

    pub(crate) fn memory_fill(&mut self) {
        self.ops.push(Op::MemoryFill);
    }

    pub(crate) fn unreachable(&mut self) {
        self.ops.push(Op::Unreachable);
    }

    pub(crate) fn block(&mut self, label: &str) {
        self.ops.push(Op::Block {
            label: strip_dollar(label).to_string(),
            ty: BlockTy::Empty,
        });
    }

    pub(crate) fn loop_(&mut self, label: &str) {
        self.ops.push(Op::Loop {
            label: strip_dollar(label).to_string(),
            ty: BlockTy::Empty,
        });
    }

    pub(crate) fn if_(&mut self) {
        self.ops.push(Op::If {
            label: String::new(),
            ty: BlockTy::Empty,
        });
    }

    pub(crate) fn if_ty(&mut self, ty: BlockTy) {
        self.ops.push(Op::If {
            label: String::new(),
            ty,
        });
    }

    pub(crate) fn else_(&mut self) {
        self.ops.push(Op::Else);
    }

    pub(crate) fn end(&mut self) {
        self.ops.push(Op::End);
    }

    pub(crate) fn br(&mut self, label: &str) {
        self.ops
            .push(Op::Br(Label::Name(strip_dollar(label).to_string())));
    }

    pub(crate) fn br_if(&mut self, label: &str) {
        self.ops
            .push(Op::BrIf(Label::Name(strip_dollar(label).to_string())));
    }

    pub(crate) fn br_depth(&mut self, d: u32) {
        self.ops.push(Op::Br(Label::Depth(d)));
    }

    pub(crate) fn br_if_depth(&mut self, d: u32) {
        self.ops.push(Op::BrIf(Label::Depth(d)));
    }

    pub(crate) fn br_table(&mut self, labels: Vec<Label>, default: Label) {
        self.ops.push(Op::BrTable { labels, default });
    }

    pub(crate) fn load(&mut self, kind: LoadKind, offset: u32) {
        let align = default_load_align(kind);
        self.ops.push(Op::Load {
            kind,
            offset,
            align,
        });
    }

    pub(crate) fn store(&mut self, kind: StoreKind, offset: u32) {
        let align = default_store_align(kind);
        self.ops.push(Op::Store {
            kind,
            offset,
            align,
        });
    }

    pub(crate) fn load_offset(&mut self, kind: LoadKind, offset: u32, align: u32) {
        self.ops.push(Op::Load {
            kind,
            offset,
            align,
        });
    }

    pub(crate) fn store_offset(&mut self, kind: StoreKind, offset: u32, align: u32) {
        self.ops.push(Op::Store {
            kind,
            offset,
            align,
        });
    }

    pub(crate) fn extract_lane(&mut self, kind: ExtractLane, lane: u8) {
        self.ops.push(Op::ExtractLane { kind, lane });
    }

    pub(crate) fn replace_lane(&mut self, kind: ReplaceLane, lane: u8) {
        self.ops.push(Op::ReplaceLane { kind, lane });
    }

    pub(crate) fn ref_func(&mut self, name: &str) {
        self.ops.push(Op::RefFunc(strip_dollar(name).to_string()));
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn set_name(&mut self, name: &str) {
        self.name = strip_dollar(name).to_string();
    }

    pub(crate) fn callees(&self) -> Vec<String> {
        let mut v = Vec::new();
        for op in &self.ops {
            match op {
                Op::Call(n) | Op::ReturnCall(n) | Op::RefFunc(n) => v.push(n.clone()),
                _ => {}
            }
        }
        v
    }

    pub(super) fn encode(
        &self,
        funcs: &IndexMap<String, u32>,
        globals: &IndexMap<String, u32>,
        tables: &IndexMap<String, u32>,
        types: &IndexMap<String, u32>,
    ) -> Function {
        super::encode::encode_func(self, funcs, globals, tables, types)
    }

    pub(super) fn ops(&self) -> &[Op] {
        &self.ops
    }

    pub(crate) fn global_names(&self) -> Vec<String> {
        let mut v = Vec::new();
        for op in &self.ops {
            match op {
                Op::GlobalGet(n) | Op::GlobalSet(n) => v.push(n.clone()),
                _ => {}
            }
        }
        v
    }

    pub(crate) fn type_names_used(&self) -> Vec<String> {
        let mut v = Vec::new();
        for op in &self.ops {
            if let Op::CallIndirect { type_name, .. } = op {
                if !type_name.is_empty() {
                    v.push(type_name.clone());
                }
            }
        }
        v
    }

    pub(super) fn extra_local_types(&self) -> Vec<ValType> {
        self.locals.iter().map(|(_, t)| *t).collect()
    }

    pub(super) fn local_index(&self, name: &str) -> u32 {
        let n = strip_dollar(name);
        if let Some(i) = self.params.iter().position(|(p, _)| p == n) {
            return i as u32;
        }
        if let Some(i) = self.locals.iter().position(|(p, _)| p == n) {
            return (self.params.len() + i) as u32;
        }
        if let Ok(i) = n.parse::<u32>() {
            return i;
        }
        crate::internal_error!("unknown local ${n} in ${}", self.name)
    }
}

#[allow(dead_code)]
fn default_load_align(k: LoadKind) -> u32 {
    match k {
        LoadKind::I32_8S | LoadKind::I32_8U | LoadKind::I64_8U => 0,
        LoadKind::I32_16S | LoadKind::I32_16U | LoadKind::I64_16U => 1,
        LoadKind::I32 | LoadKind::I64_32U | LoadKind::F32 => 2,
        LoadKind::I64 | LoadKind::F64 => 3,
        LoadKind::V128 => 4,
    }
}

#[allow(dead_code)]
fn default_store_align(k: StoreKind) -> u32 {
    match k {
        StoreKind::I32_8 | StoreKind::I64_8 => 0,
        StoreKind::I32_16 | StoreKind::I64_16 => 1,
        StoreKind::I32 | StoreKind::I64_32 | StoreKind::F32 => 2,
        StoreKind::I64 | StoreKind::F64 => 3,
        StoreKind::V128 => 4,
    }
}
