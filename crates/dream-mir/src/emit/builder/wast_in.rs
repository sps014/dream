//! Parse handwritten WAT field fragments and lower them into [`ModuleBuilder`] by name.

use super::func::{BlockTy, ExtractLane, FuncBuilder, Label, LoadKind, Op, ReplaceLane, StoreKind};
use super::{strip_dollar, ModuleBuilder, ValType};
use crate::internal_error;
use wast::core::{
    Expression, FuncKind, FunctionType, GlobalKind, InnerTypeKind, Instruction, ItemKind,
    ModuleField, ModuleKind,
};
use wast::parser::{self, ParseBuffer};
use wast::token::Index;
use wast::Wat;

pub(super) fn ingest(m: &mut ModuleBuilder, text: &str) {
    let stripped: String = text
        .lines()
        .map(|l| match l.find(";;") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return;
    }
    let wrapped = if trimmed.starts_with("(module") {
        trimmed.to_string()
    } else {
        format!("(module {trimmed})")
    };
    let buf = ParseBuffer::new(&wrapped)
        .unwrap_or_else(|e| internal_error!("runtime WAT lexer failed: {e}"));
    let wat: Wat =
        parser::parse(&buf).unwrap_or_else(|e| internal_error!("runtime WAT parse failed: {e}"));
    let Wat::Module(module) = wat else {
        internal_error!("runtime WAT must be a core module fragment");
    };
    let ModuleKind::Text(fields) = module.kind else {
        internal_error!("runtime WAT must be text, not binary");
    };
    for field in fields {
        ingest_field(m, field);
    }
}

fn ingest_field(m: &mut ModuleBuilder, field: ModuleField<'_>) {
    match field {
        ModuleField::Type(ty) => {
            let name = ty.id.map(|id| id.name().to_string());
            if let InnerTypeKind::Func(ft) = ty.def.kind {
                let (params, results) = func_sig(&ft);
                m.intern_type(name.as_deref(), params, results);
            }
        }
        ModuleField::Func(func) => {
            let name = func
                .id
                .map(|id| id.name().to_string())
                .unwrap_or_else(|| m.next_anon());
            let exports = func.exports.names.clone();
            let mut fb = FuncBuilder::new(&name);
            if let Some(inline) = func.ty.inline.as_ref() {
                apply_functype(&mut fb, inline);
            }
            match func.kind {
                FuncKind::Inline { locals, expression } => {
                    for (i, loc) in locals.iter().enumerate() {
                        let n = loc
                            .id
                            .map(|id| id.name().to_string())
                            .unwrap_or_else(|| format!("__l{i}"));
                        fb.local(&n, val_ty(loc.ty));
                    }
                    lower_expr(&mut fb, &expression);
                    let fname = fb.name.clone();
                    m.push_func(fb);
                    for e in exports {
                        m.export_func(e, &fname);
                    }
                }
                FuncKind::Import(..) => {}
            }
        }
        ModuleField::Global(g) => {
            let Some(id) = g.id else {
                return;
            };
            let ty = val_ty(g.ty.ty);
            let (init, is_f32, is_f64) = match g.kind {
                GlobalKind::Inline(expr) => const_from_expr(&expr, ty),
                GlobalKind::Import(_) => return,
            };
            m.global(id.name(), ty, g.ty.mutable, init, is_f32, is_f64);
        }
        ModuleField::Export(e) => {
            let item = idx_name(&e.item);
            match e.kind {
                wast::core::ExportKind::Func => m.export_func(e.name, &item),
                wast::core::ExportKind::Table => m.export_table(e.name, &item),
                wast::core::ExportKind::Memory => m.export_memory(e.name),
                wast::core::ExportKind::Global => m.export_global(e.name, &item),
                wast::core::ExportKind::Tag => {}
            }
        }
        ModuleField::Start(idx) => m.set_start(&idx_name(&idx)),
        ModuleField::Data(d) => {
            if let wast::core::DataKind::Active { offset, .. } = d.kind {
                let off = match offset.instrs.first() {
                    Some(Instruction::I32Const(v)) => *v as u32,
                    _ => 0,
                };
                let mut bytes = Vec::new();
                for v in &d.data {
                    v.push_onto(&mut bytes);
                }
                m.data(off, bytes);
            }
        }
        ModuleField::Import(imp) => {
            if let wast::core::ImportItems::Single { module, name, sig } = imp.items {
                if let ItemKind::Func(ty) = sig.kind {
                    let (params, results) = ty.inline.as_ref().map(func_sig).unwrap_or_default();
                    let iname = sig
                        .id
                        .map(|id| id.name().to_string())
                        .unwrap_or_else(|| name.to_string());
                    m.import_func(module, name, &iname, params, results);
                }
            }
        }
        _ => {}
    }
}

fn func_sig(ft: &FunctionType<'_>) -> (Vec<ValType>, Vec<ValType>) {
    let params = ft.params.iter().map(|(_, _, t)| val_ty(*t)).collect();
    let results = ft.results.iter().map(|t| val_ty(*t)).collect();
    (params, results)
}

fn apply_functype(f: &mut FuncBuilder, ft: &FunctionType<'_>) {
    for (i, (id, _, ty)) in ft.params.iter().enumerate() {
        let n = id
            .map(|id| id.name().to_string())
            .unwrap_or_else(|| i.to_string());
        f.param(&n, val_ty(*ty));
    }
    for ty in ft.results.iter() {
        f.result(val_ty(*ty));
    }
}

fn val_ty(t: wast::core::ValType<'_>) -> ValType {
    match t {
        wast::core::ValType::I32 => ValType::I32,
        wast::core::ValType::I64 => ValType::I64,
        wast::core::ValType::F32 => ValType::F32,
        wast::core::ValType::F64 => ValType::F64,
        wast::core::ValType::V128 => ValType::V128,
        wast::core::ValType::Ref(_) => ValType::I32,
    }
}

fn idx_name(idx: &Index<'_>) -> String {
    match idx {
        Index::Id(id) => id.name().to_string(),
        Index::Num(n, _) => n.to_string(),
    }
}

fn label_of(idx: &Index<'_>) -> Label {
    match idx {
        Index::Id(id) => Label::Name(id.name().to_string()),
        Index::Num(n, _) => Label::Depth(*n),
    }
}

fn block_info(bt: &wast::core::BlockType<'_>) -> (String, BlockTy) {
    let label = bt.label.map(|id| id.name().to_string()).unwrap_or_default();
    let ty = if let Some(inline) = &bt.ty.inline {
        match inline.results.as_ref() {
            [wast::core::ValType::I32] => BlockTy::I32,
            [wast::core::ValType::I64] => BlockTy::I64,
            [wast::core::ValType::F32] => BlockTy::F32,
            [wast::core::ValType::F64] => BlockTy::F64,
            [wast::core::ValType::V128] => BlockTy::V128,
            _ => BlockTy::Empty,
        }
    } else {
        BlockTy::Empty
    };
    (label, ty)
}

fn align_log2(align: u64) -> u32 {
    if align <= 1 {
        0
    } else {
        align.trailing_zeros()
    }
}

fn const_from_expr(expr: &Expression<'_>, ty: ValType) -> (i64, bool, bool) {
    for inst in expr.instrs.iter() {
        match inst {
            Instruction::I32Const(v) => return (*v as i64, false, false),
            Instruction::I64Const(v) => return (*v, false, false),
            Instruction::F32Const(c) => return (c.bits as i64, true, false),
            Instruction::F64Const(c) => return (c.bits as i64, false, true),
            _ => {}
        }
    }
    let _ = ty;
    (0, false, false)
}

fn lower_expr(f: &mut FuncBuilder, expr: &Expression<'_>) {
    for inst in expr.instrs.iter() {
        lower_inst(f, inst);
    }
}

fn lower_inst(f: &mut FuncBuilder, inst: &Instruction<'_>) {
    match inst {
        Instruction::Unreachable => f.push(Op::Unreachable),
        Instruction::Nop => f.push(Op::Nop),
        Instruction::Block(bt) => {
            let (label, ty) = block_info(bt);
            f.push(Op::Block { label, ty });
        }
        Instruction::Loop(bt) => {
            let (label, ty) = block_info(bt);
            f.push(Op::Loop { label, ty });
        }
        Instruction::If(bt) => {
            let (label, ty) = block_info(bt);
            f.push(Op::If { label, ty });
        }
        Instruction::Else(_) => f.push(Op::Else),
        Instruction::End(_) => f.push(Op::End),
        Instruction::Br(idx) => f.push(Op::Br(label_of(idx))),
        Instruction::BrIf(idx) => f.push(Op::BrIf(label_of(idx))),
        Instruction::BrTable(t) => f.push(Op::BrTable {
            labels: t.labels.iter().map(label_of).collect(),
            default: label_of(&t.default),
        }),
        Instruction::Return => f.push(Op::Return),
        Instruction::Call(idx) => f.call(&idx_name(idx)),
        Instruction::ReturnCall(idx) => f.return_call(&idx_name(idx)),
        Instruction::CallIndirect(ci) => {
            let ty = ci
                .ty
                .index
                .as_ref()
                .map(idx_name)
                .unwrap_or_else(String::new);
            let table = match &ci.table {
                Index::Id(id) => id.name().to_string(),
                Index::Num(_, _) => "__ft".to_string(),
            };
            f.call_indirect(&ty, &table);
        }
        Instruction::Drop => f.push(Op::Drop),
        Instruction::Select(_) => f.push(Op::Select),
        Instruction::LocalGet(idx) => f.local_get(&idx_name(idx)),
        Instruction::LocalSet(idx) => f.local_set(&idx_name(idx)),
        Instruction::LocalTee(idx) => f.local_tee(&idx_name(idx)),
        Instruction::GlobalGet(idx) => f.global_get(&idx_name(idx)),
        Instruction::GlobalSet(idx) => f.global_set(&idx_name(idx)),
        Instruction::I32Load(a) => {
            f.load_offset(LoadKind::I32, a.offset as u32, align_log2(a.align))
        }
        Instruction::I64Load(a) => {
            f.load_offset(LoadKind::I64, a.offset as u32, align_log2(a.align))
        }
        Instruction::F32Load(a) => {
            f.load_offset(LoadKind::F32, a.offset as u32, align_log2(a.align))
        }
        Instruction::F64Load(a) => {
            f.load_offset(LoadKind::F64, a.offset as u32, align_log2(a.align))
        }
        Instruction::I32Load8s(a) => {
            f.load_offset(LoadKind::I32_8S, a.offset as u32, align_log2(a.align))
        }
        Instruction::I32Load8u(a) => {
            f.load_offset(LoadKind::I32_8U, a.offset as u32, align_log2(a.align))
        }
        Instruction::I32Load16s(a) => {
            f.load_offset(LoadKind::I32_16S, a.offset as u32, align_log2(a.align))
        }
        Instruction::I32Load16u(a) => {
            f.load_offset(LoadKind::I32_16U, a.offset as u32, align_log2(a.align))
        }
        Instruction::I64Load8u(a) => {
            f.load_offset(LoadKind::I64_8U, a.offset as u32, align_log2(a.align))
        }
        Instruction::I64Load16u(a) => {
            f.load_offset(LoadKind::I64_16U, a.offset as u32, align_log2(a.align))
        }
        Instruction::I64Load32u(a) => {
            f.load_offset(LoadKind::I64_32U, a.offset as u32, align_log2(a.align))
        }
        Instruction::I32Store(a) => {
            f.store_offset(StoreKind::I32, a.offset as u32, align_log2(a.align))
        }
        Instruction::I64Store(a) => {
            f.store_offset(StoreKind::I64, a.offset as u32, align_log2(a.align))
        }
        Instruction::F32Store(a) => {
            f.store_offset(StoreKind::F32, a.offset as u32, align_log2(a.align))
        }
        Instruction::F64Store(a) => {
            f.store_offset(StoreKind::F64, a.offset as u32, align_log2(a.align))
        }
        Instruction::I32Store8(a) => {
            f.store_offset(StoreKind::I32_8, a.offset as u32, align_log2(a.align))
        }
        Instruction::I32Store16(a) => {
            f.store_offset(StoreKind::I32_16, a.offset as u32, align_log2(a.align))
        }
        Instruction::I64Store8(a) => {
            f.store_offset(StoreKind::I64_8, a.offset as u32, align_log2(a.align))
        }
        Instruction::I64Store16(a) => {
            f.store_offset(StoreKind::I64_16, a.offset as u32, align_log2(a.align))
        }
        Instruction::I64Store32(a) => {
            f.store_offset(StoreKind::I64_32, a.offset as u32, align_log2(a.align))
        }
        Instruction::V128Load(a) => {
            f.load_offset(LoadKind::V128, a.offset as u32, align_log2(a.align))
        }
        Instruction::V128Store(a) => {
            f.store_offset(StoreKind::V128, a.offset as u32, align_log2(a.align))
        }
        Instruction::MemorySize(_) => f.push(Op::MemorySize),
        Instruction::MemoryGrow(_) => f.push(Op::MemoryGrow),
        Instruction::MemoryCopy(_) => f.push(Op::MemoryCopy),
        Instruction::MemoryFill(_) => f.push(Op::MemoryFill),
        Instruction::I32Const(v) => f.i32_const(*v),
        Instruction::I64Const(v) => f.i64_const(*v),
        Instruction::F32Const(c) => f.push(Op::F32Const(c.bits)),
        Instruction::F64Const(c) => f.push(Op::F64Const(c.bits)),
        Instruction::I32Clz => f.i32_clz(),
        Instruction::I32Ctz => f.i32_ctz(),
        Instruction::I32Popcnt => f.i32_popcnt(),
        Instruction::I32Add => f.i32_add(),
        Instruction::I32Sub => f.i32_sub(),
        Instruction::I32Mul => f.i32_mul(),
        Instruction::I32DivS => f.i32_div_s(),
        Instruction::I32DivU => f.i32_div_u(),
        Instruction::I32RemS => f.i32_rem_s(),
        Instruction::I32RemU => f.i32_rem_u(),
        Instruction::I32And => f.i32_and(),
        Instruction::I32Or => f.i32_or(),
        Instruction::I32Xor => f.i32_xor(),
        Instruction::I32Shl => f.i32_shl(),
        Instruction::I32ShrS => f.i32_shr_s(),
        Instruction::I32ShrU => f.i32_shr_u(),
        Instruction::I32Rotl => f.i32_rotl(),
        Instruction::I32Rotr => f.i32_rotr(),
        Instruction::I32Eqz => f.i32_eqz(),
        Instruction::I32Eq => f.i32_eq(),
        Instruction::I32Ne => f.i32_ne(),
        Instruction::I32LtS => f.i32_lt_s(),
        Instruction::I32LtU => f.i32_lt_u(),
        Instruction::I32GtS => f.i32_gt_s(),
        Instruction::I32GtU => f.i32_gt_u(),
        Instruction::I32LeS => f.i32_le_s(),
        Instruction::I32LeU => f.i32_le_u(),
        Instruction::I32GeS => f.i32_ge_s(),
        Instruction::I32GeU => f.i32_ge_u(),
        Instruction::I64Clz => f.i64_clz(),
        Instruction::I64Ctz => f.i64_ctz(),
        Instruction::I64Popcnt => f.i64_popcnt(),
        Instruction::I64Add => f.i64_add(),
        Instruction::I64Sub => f.i64_sub(),
        Instruction::I64Mul => f.i64_mul(),
        Instruction::I64DivS => f.i64_div_s(),
        Instruction::I64DivU => f.i64_div_u(),
        Instruction::I64RemS => f.i64_rem_s(),
        Instruction::I64RemU => f.i64_rem_u(),
        Instruction::I64And => f.i64_and(),
        Instruction::I64Or => f.i64_or(),
        Instruction::I64Xor => f.i64_xor(),
        Instruction::I64Shl => f.i64_shl(),
        Instruction::I64ShrS => f.i64_shr_s(),
        Instruction::I64ShrU => f.i64_shr_u(),
        Instruction::I64Rotl => f.i64_rotl(),
        Instruction::I64Rotr => f.i64_rotr(),
        Instruction::I64Eqz => f.i64_eqz(),
        Instruction::I64Eq => f.i64_eq(),
        Instruction::I64Ne => f.i64_ne(),
        Instruction::I64LtS => f.i64_lt_s(),
        Instruction::I64LtU => f.i64_lt_u(),
        Instruction::I64GtS => f.i64_gt_s(),
        Instruction::I64GtU => f.i64_gt_u(),
        Instruction::I64LeS => f.i64_le_s(),
        Instruction::I64LeU => f.i64_le_u(),
        Instruction::I64GeS => f.i64_ge_s(),
        Instruction::I64GeU => f.i64_ge_u(),
        Instruction::F32Abs => f.f32_abs(),
        Instruction::F32Neg => f.f32_neg(),
        Instruction::F32Ceil => f.f32_ceil(),
        Instruction::F32Floor => f.f32_floor(),
        Instruction::F32Trunc => f.f32_trunc(),
        Instruction::F32Nearest => f.f32_nearest(),
        Instruction::F32Sqrt => f.f32_sqrt(),
        Instruction::F32Add => f.f32_add(),
        Instruction::F32Sub => f.f32_sub(),
        Instruction::F32Mul => f.f32_mul(),
        Instruction::F32Div => f.f32_div(),
        Instruction::F32Min => f.f32_min(),
        Instruction::F32Max => f.f32_max(),
        Instruction::F32Copysign => f.f32_copysign(),
        Instruction::F32Eq => f.f32_eq(),
        Instruction::F32Ne => f.f32_ne(),
        Instruction::F32Lt => f.f32_lt(),
        Instruction::F32Gt => f.f32_gt(),
        Instruction::F32Le => f.f32_le(),
        Instruction::F32Ge => f.f32_ge(),
        Instruction::F64Abs => f.f64_abs(),
        Instruction::F64Neg => f.f64_neg(),
        Instruction::F64Ceil => f.f64_ceil(),
        Instruction::F64Floor => f.f64_floor(),
        Instruction::F64Trunc => f.f64_trunc(),
        Instruction::F64Nearest => f.f64_nearest(),
        Instruction::F64Sqrt => f.f64_sqrt(),
        Instruction::F64Add => f.f64_add(),
        Instruction::F64Sub => f.f64_sub(),
        Instruction::F64Mul => f.f64_mul(),
        Instruction::F64Div => f.f64_div(),
        Instruction::F64Min => f.f64_min(),
        Instruction::F64Max => f.f64_max(),
        Instruction::F64Copysign => f.f64_copysign(),
        Instruction::F64Eq => f.f64_eq(),
        Instruction::F64Ne => f.f64_ne(),
        Instruction::F64Lt => f.f64_lt(),
        Instruction::F64Gt => f.f64_gt(),
        Instruction::F64Le => f.f64_le(),
        Instruction::F64Ge => f.f64_ge(),
        Instruction::I32WrapI64 => f.i32_wrap_i64(),
        Instruction::I32TruncF32S => f.i32_trunc_f32_s(),
        Instruction::I32TruncF32U => f.i32_trunc_f32_u(),
        Instruction::I32TruncF64S => f.i32_trunc_f64_s(),
        Instruction::I32TruncF64U => f.i32_trunc_f64_u(),
        Instruction::I64ExtendI32S => f.i64_extend_i32_s(),
        Instruction::I64ExtendI32U => f.i64_extend_i32_u(),
        Instruction::I64TruncF32S => f.i64_trunc_f32_s(),
        Instruction::I64TruncF32U => f.i64_trunc_f32_u(),
        Instruction::I64TruncF64S => f.i64_trunc_f64_s(),
        Instruction::I64TruncF64U => f.i64_trunc_f64_u(),
        Instruction::F32ConvertI32S => f.f32_convert_i32_s(),
        Instruction::F32ConvertI32U => f.f32_convert_i32_u(),
        Instruction::F32ConvertI64S => f.f32_convert_i64_s(),
        Instruction::F32ConvertI64U => f.f32_convert_i64_u(),
        Instruction::F32DemoteF64 => f.f32_demote_f64(),
        Instruction::F64ConvertI32S => f.f64_convert_i32_s(),
        Instruction::F64ConvertI32U => f.f64_convert_i32_u(),
        Instruction::F64ConvertI64S => f.f64_convert_i64_s(),
        Instruction::F64ConvertI64U => f.f64_convert_i64_u(),
        Instruction::F64PromoteF32 => f.f64_promote_f32(),
        Instruction::I32ReinterpretF32 => f.i32_reinterpret_f32(),
        Instruction::I64ReinterpretF64 => f.i64_reinterpret_f64(),
        Instruction::F32ReinterpretI32 => f.f32_reinterpret_i32(),
        Instruction::F64ReinterpretI64 => f.f64_reinterpret_i64(),
        Instruction::I32Extend8S => f.i32_extend8_s(),
        Instruction::I32Extend16S => f.i32_extend16_s(),
        Instruction::I64Extend8S => f.i64_extend8_s(),
        Instruction::I64Extend16S => f.i64_extend16_s(),
        Instruction::I64Extend32S => f.i64_extend32_s(),
        Instruction::I32AtomicLoad(a) => f.push(Op::AtomicLoad {
            offset: a.offset as u32,
        }),
        Instruction::I32AtomicStore(a) => f.push(Op::AtomicStore {
            offset: a.offset as u32,
        }),
        Instruction::I32AtomicRmwAdd(a) => f.push(Op::AtomicRmwAdd {
            offset: a.offset as u32,
        }),
        Instruction::I32AtomicRmwSub(a) => f.push(Op::AtomicRmwSub {
            offset: a.offset as u32,
        }),
        Instruction::I32AtomicRmwCmpxchg(a) => f.push(Op::AtomicRmwCmpxchg {
            offset: a.offset as u32,
        }),
        Instruction::MemoryAtomicNotify(a) => f.push(Op::MemoryAtomicNotify {
            offset: a.offset as u32,
        }),
        Instruction::MemoryAtomicWait32(a) => f.push(Op::MemoryAtomicWait32 {
            offset: a.offset as u32,
        }),
        Instruction::RefFunc(idx) => f.ref_func(&idx_name(idx)),
        Instruction::V128Const(c) => f.push(Op::V128Const(c.to_le_bytes())),
        Instruction::I8x16Shuffle(s) => f.push(Op::Shuffle(s.lanes)),
        Instruction::I8x16ExtractLaneS(a) => f.extract_lane(ExtractLane::I8x16S, a.lane),
        Instruction::I8x16ExtractLaneU(a) => f.extract_lane(ExtractLane::I8x16U, a.lane),
        Instruction::I8x16ReplaceLane(a) => f.replace_lane(ReplaceLane::I8x16, a.lane),
        Instruction::I16x8ExtractLaneS(a) => f.extract_lane(ExtractLane::I16x8S, a.lane),
        Instruction::I16x8ExtractLaneU(a) => f.extract_lane(ExtractLane::I16x8U, a.lane),
        Instruction::I16x8ReplaceLane(a) => f.replace_lane(ReplaceLane::I16x8, a.lane),
        Instruction::I32x4ExtractLane(a) => f.extract_lane(ExtractLane::I32x4, a.lane),
        Instruction::I32x4ReplaceLane(a) => f.replace_lane(ReplaceLane::I32x4, a.lane),
        Instruction::I64x2ExtractLane(a) => f.extract_lane(ExtractLane::I64x2, a.lane),
        Instruction::I64x2ReplaceLane(a) => f.replace_lane(ReplaceLane::I64x2, a.lane),
        Instruction::F32x4ExtractLane(a) => f.extract_lane(ExtractLane::F32x4, a.lane),
        Instruction::F32x4ReplaceLane(a) => f.replace_lane(ReplaceLane::F32x4, a.lane),
        Instruction::F64x2ExtractLane(a) => f.extract_lane(ExtractLane::F64x2, a.lane),
        Instruction::F64x2ReplaceLane(a) => f.replace_lane(ReplaceLane::F64x2, a.lane),
        Instruction::I32TruncSatF32S => f.i32_trunc_sat_f32_s(),
        Instruction::I32TruncSatF32U => f.i32_trunc_sat_f32_u(),
        Instruction::I32TruncSatF64S => f.i32_trunc_sat_f64_s(),
        Instruction::I32TruncSatF64U => f.i32_trunc_sat_f64_u(),
        Instruction::I64TruncSatF32S => f.i64_trunc_sat_f32_s(),
        Instruction::I64TruncSatF32U => f.i64_trunc_sat_f32_u(),
        Instruction::I64TruncSatF64S => f.i64_trunc_sat_f64_s(),
        Instruction::I64TruncSatF64U => f.i64_trunc_sat_f64_u(),
        Instruction::I8x16Swizzle => f.i8x16_swizzle(),
        Instruction::I8x16Splat => f.i8x16_splat(),
        Instruction::I16x8Splat => f.i16x8_splat(),
        Instruction::I32x4Splat => f.i32x4_splat(),
        Instruction::I64x2Splat => f.i64x2_splat(),
        Instruction::F32x4Splat => f.f32x4_splat(),
        Instruction::F64x2Splat => f.f64x2_splat(),
        Instruction::I8x16Eq => f.i8x16_eq(),
        Instruction::I8x16Ne => f.i8x16_ne(),
        Instruction::I8x16LtS => f.i8x16_lt_s(),
        Instruction::I8x16LtU => f.i8x16_lt_u(),
        Instruction::I8x16GtS => f.i8x16_gt_s(),
        Instruction::I8x16GtU => f.i8x16_gt_u(),
        Instruction::I8x16LeS => f.i8x16_le_s(),
        Instruction::I8x16LeU => f.i8x16_le_u(),
        Instruction::I8x16GeS => f.i8x16_ge_s(),
        Instruction::I8x16GeU => f.i8x16_ge_u(),
        Instruction::I16x8Eq => f.i16x8_eq(),
        Instruction::I16x8Ne => f.i16x8_ne(),
        Instruction::I16x8LtS => f.i16x8_lt_s(),
        Instruction::I16x8LtU => f.i16x8_lt_u(),
        Instruction::I16x8GtS => f.i16x8_gt_s(),
        Instruction::I16x8GtU => f.i16x8_gt_u(),
        Instruction::I16x8LeS => f.i16x8_le_s(),
        Instruction::I16x8LeU => f.i16x8_le_u(),
        Instruction::I16x8GeS => f.i16x8_ge_s(),
        Instruction::I16x8GeU => f.i16x8_ge_u(),
        Instruction::I32x4Eq => f.i32x4_eq(),
        Instruction::I32x4Ne => f.i32x4_ne(),
        Instruction::I32x4LtS => f.i32x4_lt_s(),
        Instruction::I32x4LtU => f.i32x4_lt_u(),
        Instruction::I32x4GtS => f.i32x4_gt_s(),
        Instruction::I32x4GtU => f.i32x4_gt_u(),
        Instruction::I32x4LeS => f.i32x4_le_s(),
        Instruction::I32x4LeU => f.i32x4_le_u(),
        Instruction::I32x4GeS => f.i32x4_ge_s(),
        Instruction::I32x4GeU => f.i32x4_ge_u(),
        Instruction::I64x2Eq => f.i64x2_eq(),
        Instruction::I64x2Ne => f.i64x2_ne(),
        Instruction::I64x2LtS => f.i64x2_lt_s(),
        Instruction::I64x2GtS => f.i64x2_gt_s(),
        Instruction::I64x2LeS => f.i64x2_le_s(),
        Instruction::I64x2GeS => f.i64x2_ge_s(),
        Instruction::F32x4Eq => f.f32x4_eq(),
        Instruction::F32x4Ne => f.f32x4_ne(),
        Instruction::F32x4Lt => f.f32x4_lt(),
        Instruction::F32x4Gt => f.f32x4_gt(),
        Instruction::F32x4Le => f.f32x4_le(),
        Instruction::F32x4Ge => f.f32x4_ge(),
        Instruction::F64x2Eq => f.f64x2_eq(),
        Instruction::F64x2Ne => f.f64x2_ne(),
        Instruction::F64x2Lt => f.f64x2_lt(),
        Instruction::F64x2Gt => f.f64x2_gt(),
        Instruction::F64x2Le => f.f64x2_le(),
        Instruction::F64x2Ge => f.f64x2_ge(),
        Instruction::V128Not => f.v128_not(),
        Instruction::V128And => f.v128_and(),
        Instruction::V128Andnot => f.v128_andnot(),
        Instruction::V128Or => f.v128_or(),
        Instruction::V128Xor => f.v128_xor(),
        Instruction::V128Bitselect => f.v128_bitselect(),
        Instruction::V128AnyTrue => f.v128_any_true(),
        Instruction::I8x16Abs => f.i8x16_abs(),
        Instruction::I8x16Neg => f.i8x16_neg(),
        Instruction::I8x16Popcnt => f.i8x16_popcnt(),
        Instruction::I8x16AllTrue => f.i8x16_all_true(),
        Instruction::I8x16Bitmask => f.i8x16_bitmask(),
        Instruction::I8x16NarrowI16x8S => f.i8x16_narrow_i16x8_s(),
        Instruction::I8x16NarrowI16x8U => f.i8x16_narrow_i16x8_u(),
        Instruction::I8x16Shl => f.i8x16_shl(),
        Instruction::I8x16ShrS => f.i8x16_shr_s(),
        Instruction::I8x16ShrU => f.i8x16_shr_u(),
        Instruction::I8x16Add => f.i8x16_add(),
        Instruction::I8x16AddSatS => f.i8x16_add_sat_s(),
        Instruction::I8x16AddSatU => f.i8x16_add_sat_u(),
        Instruction::I8x16Sub => f.i8x16_sub(),
        Instruction::I8x16SubSatS => f.i8x16_sub_sat_s(),
        Instruction::I8x16SubSatU => f.i8x16_sub_sat_u(),
        Instruction::I8x16MinS => f.i8x16_min_s(),
        Instruction::I8x16MinU => f.i8x16_min_u(),
        Instruction::I8x16MaxS => f.i8x16_max_s(),
        Instruction::I8x16MaxU => f.i8x16_max_u(),
        Instruction::I8x16AvgrU => f.i8x16_avgr_u(),
        Instruction::I16x8ExtAddPairwiseI8x16S => f.i16x8_extadd_pairwise_i8x16_s(),
        Instruction::I16x8ExtAddPairwiseI8x16U => f.i16x8_extadd_pairwise_i8x16_u(),
        Instruction::I16x8Abs => f.i16x8_abs(),
        Instruction::I16x8Neg => f.i16x8_neg(),
        Instruction::I16x8Q15MulrSatS => f.i16x8_q15mulr_sat_s(),
        Instruction::I16x8AllTrue => f.i16x8_all_true(),
        Instruction::I16x8Bitmask => f.i16x8_bitmask(),
        Instruction::I16x8NarrowI32x4S => f.i16x8_narrow_i32x4_s(),
        Instruction::I16x8NarrowI32x4U => f.i16x8_narrow_i32x4_u(),
        Instruction::I16x8ExtendLowI8x16S => f.i16x8_extend_low_i8x16_s(),
        Instruction::I16x8ExtendHighI8x16S => f.i16x8_extend_high_i8x16_s(),
        Instruction::I16x8ExtendLowI8x16U => f.i16x8_extend_low_i8x16_u(),
        Instruction::I16x8ExtendHighI8x16u => f.i16x8_extend_high_i8x16_u(),
        Instruction::I16x8Shl => f.i16x8_shl(),
        Instruction::I16x8ShrS => f.i16x8_shr_s(),
        Instruction::I16x8ShrU => f.i16x8_shr_u(),
        Instruction::I16x8Add => f.i16x8_add(),
        Instruction::I16x8AddSatS => f.i16x8_add_sat_s(),
        Instruction::I16x8AddSatU => f.i16x8_add_sat_u(),
        Instruction::I16x8Sub => f.i16x8_sub(),
        Instruction::I16x8SubSatS => f.i16x8_sub_sat_s(),
        Instruction::I16x8SubSatU => f.i16x8_sub_sat_u(),
        Instruction::I16x8Mul => f.i16x8_mul(),
        Instruction::I16x8MinS => f.i16x8_min_s(),
        Instruction::I16x8MinU => f.i16x8_min_u(),
        Instruction::I16x8MaxS => f.i16x8_max_s(),
        Instruction::I16x8MaxU => f.i16x8_max_u(),
        Instruction::I16x8AvgrU => f.i16x8_avgr_u(),
        Instruction::I16x8ExtMulLowI8x16S => f.i16x8_extmul_low_i8x16_s(),
        Instruction::I16x8ExtMulHighI8x16S => f.i16x8_extmul_high_i8x16_s(),
        Instruction::I16x8ExtMulLowI8x16U => f.i16x8_extmul_low_i8x16_u(),
        Instruction::I16x8ExtMulHighI8x16U => f.i16x8_extmul_high_i8x16_u(),
        Instruction::I32x4ExtAddPairwiseI16x8S => f.i32x4_extadd_pairwise_i16x8_s(),
        Instruction::I32x4ExtAddPairwiseI16x8U => f.i32x4_extadd_pairwise_i16x8_u(),
        Instruction::I32x4Abs => f.i32x4_abs(),
        Instruction::I32x4Neg => f.i32x4_neg(),
        Instruction::I32x4AllTrue => f.i32x4_all_true(),
        Instruction::I32x4Bitmask => f.i32x4_bitmask(),
        Instruction::I32x4ExtendLowI16x8S => f.i32x4_extend_low_i16x8_s(),
        Instruction::I32x4ExtendHighI16x8S => f.i32x4_extend_high_i16x8_s(),
        Instruction::I32x4ExtendLowI16x8U => f.i32x4_extend_low_i16x8_u(),
        Instruction::I32x4ExtendHighI16x8U => f.i32x4_extend_high_i16x8_u(),
        Instruction::I32x4Shl => f.i32x4_shl(),
        Instruction::I32x4ShrS => f.i32x4_shr_s(),
        Instruction::I32x4ShrU => f.i32x4_shr_u(),
        Instruction::I32x4Add => f.i32x4_add(),
        Instruction::I32x4Sub => f.i32x4_sub(),
        Instruction::I32x4Mul => f.i32x4_mul(),
        Instruction::I32x4MinS => f.i32x4_min_s(),
        Instruction::I32x4MinU => f.i32x4_min_u(),
        Instruction::I32x4MaxS => f.i32x4_max_s(),
        Instruction::I32x4MaxU => f.i32x4_max_u(),
        Instruction::I32x4DotI16x8S => f.i32x4_dot_i16x8_s(),
        Instruction::I32x4ExtMulLowI16x8S => f.i32x4_extmul_low_i16x8_s(),
        Instruction::I32x4ExtMulHighI16x8S => f.i32x4_extmul_high_i16x8_s(),
        Instruction::I32x4ExtMulLowI16x8U => f.i32x4_extmul_low_i16x8_u(),
        Instruction::I32x4ExtMulHighI16x8U => f.i32x4_extmul_high_i16x8_u(),
        Instruction::I64x2Abs => f.i64x2_abs(),
        Instruction::I64x2Neg => f.i64x2_neg(),
        Instruction::I64x2AllTrue => f.i64x2_all_true(),
        Instruction::I64x2Bitmask => f.i64x2_bitmask(),
        Instruction::I64x2ExtendLowI32x4S => f.i64x2_extend_low_i32x4_s(),
        Instruction::I64x2ExtendHighI32x4S => f.i64x2_extend_high_i32x4_s(),
        Instruction::I64x2ExtendLowI32x4U => f.i64x2_extend_low_i32x4_u(),
        Instruction::I64x2ExtendHighI32x4U => f.i64x2_extend_high_i32x4_u(),
        Instruction::I64x2Shl => f.i64x2_shl(),
        Instruction::I64x2ShrS => f.i64x2_shr_s(),
        Instruction::I64x2ShrU => f.i64x2_shr_u(),
        Instruction::I64x2Add => f.i64x2_add(),
        Instruction::I64x2Sub => f.i64x2_sub(),
        Instruction::I64x2Mul => f.i64x2_mul(),
        Instruction::I64x2ExtMulLowI32x4S => f.i64x2_extmul_low_i32x4_s(),
        Instruction::I64x2ExtMulHighI32x4S => f.i64x2_extmul_high_i32x4_s(),
        Instruction::I64x2ExtMulLowI32x4U => f.i64x2_extmul_low_i32x4_u(),
        Instruction::I64x2ExtMulHighI32x4U => f.i64x2_extmul_high_i32x4_u(),
        Instruction::F32x4Ceil => f.f32x4_ceil(),
        Instruction::F32x4Floor => f.f32x4_floor(),
        Instruction::F32x4Trunc => f.f32x4_trunc(),
        Instruction::F32x4Nearest => f.f32x4_nearest(),
        Instruction::F32x4Abs => f.f32x4_abs(),
        Instruction::F32x4Neg => f.f32x4_neg(),
        Instruction::F32x4Sqrt => f.f32x4_sqrt(),
        Instruction::F32x4Add => f.f32x4_add(),
        Instruction::F32x4Sub => f.f32x4_sub(),
        Instruction::F32x4Mul => f.f32x4_mul(),
        Instruction::F32x4Div => f.f32x4_div(),
        Instruction::F32x4Min => f.f32x4_min(),
        Instruction::F32x4Max => f.f32x4_max(),
        Instruction::F32x4PMin => f.f32x4_pmin(),
        Instruction::F32x4PMax => f.f32x4_pmax(),
        Instruction::F64x2Ceil => f.f64x2_ceil(),
        Instruction::F64x2Floor => f.f64x2_floor(),
        Instruction::F64x2Trunc => f.f64x2_trunc(),
        Instruction::F64x2Nearest => f.f64x2_nearest(),
        Instruction::F64x2Abs => f.f64x2_abs(),
        Instruction::F64x2Neg => f.f64x2_neg(),
        Instruction::F64x2Sqrt => f.f64x2_sqrt(),
        Instruction::F64x2Add => f.f64x2_add(),
        Instruction::F64x2Sub => f.f64x2_sub(),
        Instruction::F64x2Mul => f.f64x2_mul(),
        Instruction::F64x2Div => f.f64x2_div(),
        Instruction::F64x2Min => f.f64x2_min(),
        Instruction::F64x2Max => f.f64x2_max(),
        Instruction::F64x2PMin => f.f64x2_pmin(),
        Instruction::F64x2PMax => f.f64x2_pmax(),
        Instruction::I32x4TruncSatF32x4S => f.i32x4_trunc_sat_f32x4_s(),
        Instruction::I32x4TruncSatF32x4U => f.i32x4_trunc_sat_f32x4_u(),
        Instruction::F32x4ConvertI32x4S => f.f32x4_convert_i32x4_s(),
        Instruction::F32x4ConvertI32x4U => f.f32x4_convert_i32x4_u(),
        Instruction::I32x4TruncSatF64x2SZero => f.i32x4_trunc_sat_f64x2_s_zero(),
        Instruction::I32x4TruncSatF64x2UZero => f.i32x4_trunc_sat_f64x2_u_zero(),
        Instruction::F64x2ConvertLowI32x4S => f.f64x2_convert_low_i32x4_s(),
        Instruction::F64x2ConvertLowI32x4U => f.f64x2_convert_low_i32x4_u(),
        Instruction::F32x4DemoteF64x2Zero => f.f32x4_demote_f64x2_zero(),
        Instruction::F64x2PromoteLowF32x4 => f.f64x2_promote_low_f32x4(),
        Instruction::I8x16RelaxedSwizzle => f.i8x16_relaxed_swizzle(),
        Instruction::I32x4RelaxedTruncF32x4S => f.i32x4_relaxed_trunc_f32x4_s(),
        Instruction::I32x4RelaxedTruncF32x4U => f.i32x4_relaxed_trunc_f32x4_u(),
        Instruction::I32x4RelaxedTruncF64x2SZero => f.i32x4_relaxed_trunc_f64x2_s_zero(),
        Instruction::I32x4RelaxedTruncF64x2UZero => f.i32x4_relaxed_trunc_f64x2_u_zero(),
        Instruction::F32x4RelaxedMadd => f.f32x4_relaxed_madd(),
        Instruction::F32x4RelaxedNmadd => f.f32x4_relaxed_nmadd(),
        Instruction::F64x2RelaxedMadd => f.f64x2_relaxed_madd(),
        Instruction::F64x2RelaxedNmadd => f.f64x2_relaxed_nmadd(),
        Instruction::I8x16RelaxedLaneselect => f.i8x16_relaxed_laneselect(),
        Instruction::I16x8RelaxedLaneselect => f.i16x8_relaxed_laneselect(),
        Instruction::I32x4RelaxedLaneselect => f.i32x4_relaxed_laneselect(),
        Instruction::I64x2RelaxedLaneselect => f.i64x2_relaxed_laneselect(),
        Instruction::F32x4RelaxedMin => f.f32x4_relaxed_min(),
        Instruction::F32x4RelaxedMax => f.f32x4_relaxed_max(),
        Instruction::F64x2RelaxedMin => f.f64x2_relaxed_min(),
        Instruction::F64x2RelaxedMax => f.f64x2_relaxed_max(),
        Instruction::I16x8RelaxedQ15mulrS => f.i16x8_relaxed_q15mulr_s(),
        Instruction::I16x8RelaxedDotI8x16I7x16S => f.i16x8_relaxed_dot_i8x16_i7x16_s(),
        Instruction::I32x4RelaxedDotI8x16I7x16AddS => f.i32x4_relaxed_dot_i8x16_i7x16_add_s(),
        other => internal_error!("unsupported runtime WAT instruction: {other:?}"),
    }
}

#[allow(dead_code)]
fn _strip(s: &str) -> &str {
    strip_dollar(s)
}
