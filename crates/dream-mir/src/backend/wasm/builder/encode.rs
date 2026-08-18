//! Lower [`Op`] to `wasm_encoder` bytes.

use super::func::{BlockTy, ExtractLane, FuncBuilder, Label, LoadKind, Op, ReplaceLane, StoreKind};
use super::inst::encode_nullary;
use crate::internal_error;
use indexmap::IndexMap;
use wasm_encoder::{BlockType, Function, Ieee32, Ieee64, InstructionSink, MemArg, ValType};

pub(super) fn encode_func(
    f: &FuncBuilder,
    funcs: &IndexMap<String, u32>,
    globals: &IndexMap<String, u32>,
    tables: &IndexMap<String, u32>,
    types: &IndexMap<String, u32>,
) -> Function {
    let mut func = Function::new_with_locals_types(f.extra_local_types());
    let mut labels: Vec<String> = Vec::new();
    {
        let mut sink = func.instructions();
        for op in f.ops() {
            encode_op(&mut sink, op, f, funcs, globals, tables, types, &mut labels);
        }
        // Implicit end of function body.
        sink.end();
    }
    func
}

fn resolve_label(labels: &[String], lab: &Label) -> u32 {
    match lab {
        Label::Depth(d) => *d,
        Label::Name(n) => {
            if let Ok(d) = n.parse::<u32>() {
                return d;
            }
            labels
                .iter()
                .rev()
                .position(|l| l == n || (n.is_empty() && l.is_empty()))
                .unwrap_or_else(|| internal_error!("unknown branch label ${n}")) as u32
        }
    }
}

fn resolve_name(map: &IndexMap<String, u32>, name: &str, kind: &str) -> u32 {
    map.get(name)
        .copied()
        .unwrap_or_else(|| internal_error!("unknown {kind} ${name}"))
}

fn block_ty(ty: BlockTy) -> BlockType {
    match ty {
        BlockTy::Empty => BlockType::Empty,
        BlockTy::I32 => BlockType::Result(ValType::I32),
        BlockTy::I64 => BlockType::Result(ValType::I64),
        BlockTy::F32 => BlockType::Result(ValType::F32),
        BlockTy::F64 => BlockType::Result(ValType::F64),
        BlockTy::V128 => BlockType::Result(ValType::V128),
    }
}

fn mem(offset: u32, align: u32) -> MemArg {
    MemArg {
        offset: offset as u64,
        align,
        memory_index: 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn encode_op(
    sink: &mut InstructionSink<'_>,
    op: &Op,
    f: &FuncBuilder,
    funcs: &IndexMap<String, u32>,
    globals: &IndexMap<String, u32>,
    tables: &IndexMap<String, u32>,
    types: &IndexMap<String, u32>,
    labels: &mut Vec<String>,
) {
    match op {
        Op::Unreachable => {
            sink.unreachable();
        }
        Op::Nop => {
            sink.nop();
        }
        Op::Block { label, ty } => {
            labels.push(label.clone());
            sink.block(block_ty(*ty));
        }
        Op::Loop { label, ty } => {
            labels.push(label.clone());
            sink.loop_(block_ty(*ty));
        }
        Op::If { label, ty } => {
            labels.push(label.clone());
            sink.if_(block_ty(*ty));
        }
        Op::Else => {
            sink.else_();
        }
        Op::End => {
            labels.pop();
            sink.end();
        }
        Op::Br(l) => {
            sink.br(resolve_label(labels, l));
        }
        Op::BrIf(l) => {
            sink.br_if(resolve_label(labels, l));
        }
        Op::BrTable {
            labels: ls,
            default,
        } => {
            let targets: Vec<u32> = ls.iter().map(|l| resolve_label(labels, l)).collect();
            sink.br_table(targets, resolve_label(labels, default));
        }
        Op::Return => {
            sink.return_();
        }
        Op::Call(n) => {
            sink.call(resolve_name(funcs, n, "function"));
        }
        Op::ReturnCall(n) => {
            sink.return_call(resolve_name(funcs, n, "function"));
        }
        Op::CallIndirect { type_name, table } => {
            let ty = resolve_name(types, type_name, "type");
            let tab = if table.is_empty() {
                0
            } else {
                resolve_name(tables, table, "table")
            };
            sink.call_indirect(tab, ty);
        }
        Op::Drop => {
            sink.drop();
        }
        Op::Select => {
            sink.select();
        }
        Op::LocalGet(n) => {
            sink.local_get(f.local_index(n));
        }
        Op::LocalSet(n) => {
            sink.local_set(f.local_index(n));
        }
        Op::LocalTee(n) => {
            sink.local_tee(f.local_index(n));
        }
        Op::GlobalGet(n) => {
            sink.global_get(resolve_name(globals, n, "global"));
        }
        Op::GlobalSet(n) => {
            sink.global_set(resolve_name(globals, n, "global"));
        }
        Op::Load {
            kind,
            offset,
            align,
        } => encode_load(sink, *kind, mem(*offset, *align)),
        Op::Store {
            kind,
            offset,
            align,
        } => encode_store(sink, *kind, mem(*offset, *align)),
        Op::MemorySize => {
            sink.memory_size(0);
        }
        Op::MemoryGrow => {
            sink.memory_grow(0);
        }
        Op::MemoryCopy => {
            sink.memory_copy(0, 0);
        }
        Op::MemoryFill => {
            sink.memory_fill(0);
        }
        Op::I32Const(v) => {
            sink.i32_const(*v);
        }
        Op::I64Const(v) => {
            sink.i64_const(*v);
        }
        Op::F32Const(b) => {
            sink.f32_const(Ieee32::new(*b));
        }
        Op::F64Const(b) => {
            sink.f64_const(Ieee64::new(*b));
        }
        Op::Nullary(op) => encode_nullary(sink, *op),
        Op::AtomicLoad { offset } => {
            sink.i32_atomic_load(mem(*offset, 2));
        }
        Op::AtomicStore { offset } => {
            sink.i32_atomic_store(mem(*offset, 2));
        }
        Op::AtomicRmwAdd { offset } => {
            sink.i32_atomic_rmw_add(mem(*offset, 2));
        }
        Op::AtomicRmwSub { offset } => {
            sink.i32_atomic_rmw_sub(mem(*offset, 2));
        }
        Op::AtomicRmwCmpxchg { offset } => {
            sink.i32_atomic_rmw_cmpxchg(mem(*offset, 2));
        }
        Op::MemoryAtomicNotify { offset } => {
            sink.memory_atomic_notify(mem(*offset, 2));
        }
        Op::MemoryAtomicWait32 { offset } => {
            sink.memory_atomic_wait32(mem(*offset, 2));
        }
        Op::RefFunc(n) => {
            sink.ref_func(resolve_name(funcs, n, "function"));
        }
        Op::ExtractLane { kind, lane } => encode_extract(sink, *kind, *lane),
        Op::ReplaceLane { kind, lane } => encode_replace(sink, *kind, *lane),
        Op::V128Const(bytes) => {
            sink.v128_const(i128::from_le_bytes(*bytes));
        }
        Op::Shuffle(lanes) => {
            sink.i8x16_shuffle(*lanes);
        }
    }
}

fn encode_load(sink: &mut InstructionSink<'_>, kind: LoadKind, m: MemArg) {
    match kind {
        LoadKind::I32 => sink.i32_load(m),
        LoadKind::I32_8S => sink.i32_load8_s(m),
        LoadKind::I32_8U => sink.i32_load8_u(m),
        LoadKind::I32_16S => sink.i32_load16_s(m),
        LoadKind::I32_16U => sink.i32_load16_u(m),
        LoadKind::I64 => sink.i64_load(m),
        LoadKind::I64_8U => sink.i64_load8_u(m),
        LoadKind::I64_16U => sink.i64_load16_u(m),
        LoadKind::I64_32U => sink.i64_load32_u(m),
        LoadKind::F32 => sink.f32_load(m),
        LoadKind::F64 => sink.f64_load(m),
        LoadKind::V128 => sink.v128_load(m),
    };
}

fn encode_store(sink: &mut InstructionSink<'_>, kind: StoreKind, m: MemArg) {
    match kind {
        StoreKind::I32 => sink.i32_store(m),
        StoreKind::I32_8 => sink.i32_store8(m),
        StoreKind::I32_16 => sink.i32_store16(m),
        StoreKind::I64 => sink.i64_store(m),
        StoreKind::I64_8 => sink.i64_store8(m),
        StoreKind::I64_16 => sink.i64_store16(m),
        StoreKind::I64_32 => sink.i64_store32(m),
        StoreKind::F32 => sink.f32_store(m),
        StoreKind::F64 => sink.f64_store(m),
        StoreKind::V128 => sink.v128_store(m),
    };
}

fn encode_extract(sink: &mut InstructionSink<'_>, kind: ExtractLane, lane: u8) {
    match kind {
        ExtractLane::I8x16S => sink.i8x16_extract_lane_s(lane),
        ExtractLane::I8x16U => sink.i8x16_extract_lane_u(lane),
        ExtractLane::I16x8S => sink.i16x8_extract_lane_s(lane),
        ExtractLane::I16x8U => sink.i16x8_extract_lane_u(lane),
        ExtractLane::I32x4 => sink.i32x4_extract_lane(lane),
        ExtractLane::I64x2 => sink.i64x2_extract_lane(lane),
        ExtractLane::F32x4 => sink.f32x4_extract_lane(lane),
        ExtractLane::F64x2 => sink.f64x2_extract_lane(lane),
    };
}

fn encode_replace(sink: &mut InstructionSink<'_>, kind: ReplaceLane, lane: u8) {
    match kind {
        ReplaceLane::I8x16 => sink.i8x16_replace_lane(lane),
        ReplaceLane::I16x8 => sink.i16x8_replace_lane(lane),
        ReplaceLane::I32x4 => sink.i32x4_replace_lane(lane),
        ReplaceLane::I64x2 => sink.i64x2_replace_lane(lane),
        ReplaceLane::F32x4 => sink.f32x4_replace_lane(lane),
        ReplaceLane::F64x2 => sink.f64x2_replace_lane(lane),
    };
}
