//! Parameterless WASM operators. Encode maps each variant onto `InstructionSink` with no string table.

use wasm_encoder::InstructionSink;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Nullary {
    I32Eqz,
    I32Eq,
    I32Ne,
    I32LtS,
    I32LtU,
    I32GtS,
    I32GtU,
    I32LeS,
    I32LeU,
    I32GeS,
    I32GeU,
    I64Eqz,
    I64Eq,
    I64Ne,
    I64LtS,
    I64LtU,
    I64GtS,
    I64GtU,
    I64LeS,
    I64LeU,
    I64GeS,
    I64GeU,
    F32Eq,
    F32Ne,
    F32Lt,
    F32Gt,
    F32Le,
    F32Ge,
    F64Eq,
    F64Ne,
    F64Lt,
    F64Gt,
    F64Le,
    F64Ge,
    I32Clz,
    I32Ctz,
    I32Popcnt,
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32DivU,
    I32RemS,
    I32RemU,
    I32And,
    I32Or,
    I32Xor,
    I32Shl,
    I32ShrS,
    I32ShrU,
    I32Rotl,
    I32Rotr,
    I64Clz,
    I64Ctz,
    I64Popcnt,
    I64Add,
    I64Sub,
    I64Mul,
    I64DivS,
    I64DivU,
    I64RemS,
    I64RemU,
    I64And,
    I64Or,
    I64Xor,
    I64Shl,
    I64ShrS,
    I64ShrU,
    I64Rotl,
    I64Rotr,
    F32Abs,
    F32Neg,
    F32Ceil,
    F32Floor,
    F32Trunc,
    F32Nearest,
    F32Sqrt,
    F32Add,
    F32Sub,
    F32Mul,
    F32Div,
    F32Min,
    F32Max,
    F32Copysign,
    F64Abs,
    F64Neg,
    F64Ceil,
    F64Floor,
    F64Trunc,
    F64Nearest,
    F64Sqrt,
    F64Add,
    F64Sub,
    F64Mul,
    F64Div,
    F64Min,
    F64Max,
    F64Copysign,
    I32WrapI64,
    I32TruncF32S,
    I32TruncF32U,
    I32TruncF64S,
    I32TruncF64U,
    I32TruncSatF32S,
    I32TruncSatF32U,
    I32TruncSatF64S,
    I32TruncSatF64U,
    I64TruncSatF32S,
    I64TruncSatF32U,
    I64TruncSatF64S,
    I64TruncSatF64U,
    I64ExtendI32S,
    I64ExtendI32U,
    I64TruncF32S,
    I64TruncF32U,
    I64TruncF64S,
    I64TruncF64U,
    F32ConvertI32S,
    F32ConvertI32U,
    F32ConvertI64S,
    F32ConvertI64U,
    F32DemoteF64,
    F64ConvertI32S,
    F64ConvertI32U,
    F64ConvertI64S,
    F64ConvertI64U,
    F64PromoteF32,
    I32ReinterpretF32,
    I64ReinterpretF64,
    F32ReinterpretI32,
    F64ReinterpretI64,
    I32EXTEND8S,
    I32EXTEND16S,
    I64EXTEND8S,
    I64EXTEND16S,
    I64EXTEND32S,
    I8x16Add,
    I8x16Sub,
    I8x16MinS,
    I8x16MaxS,
    I16x8Add,
    I16x8Sub,
    I16x8Mul,
    I16x8MinS,
    I16x8MaxS,
    I32x4Add,
    I32x4Sub,
    I32x4Mul,
    I32x4MinS,
    I32x4MaxS,
    I64x2Add,
    I64x2Sub,
    I64x2Mul,
    F32x4Add,
    F32x4Sub,
    F32x4Mul,
    F32x4Min,
    F32x4Max,
    F64x2Add,
    F64x2Sub,
    F64x2Mul,
    F64x2Min,
    F64x2Max,
    I8x16Splat,
    I16x8Splat,
    I32x4Splat,
    I64x2Splat,
    F32x4Splat,
    F64x2Splat,
    I8x16Swizzle,
    I8x16Eq,
    I8x16Ne,
    I8x16LtS,
    I8x16LtU,
    I8x16GtS,
    I8x16GtU,
    I8x16LeS,
    I8x16LeU,
    I8x16GeS,
    I8x16GeU,
    I16x8Eq,
    I16x8Ne,
    I16x8LtS,
    I16x8LtU,
    I16x8GtS,
    I16x8GtU,
    I16x8LeS,
    I16x8LeU,
    I16x8GeS,
    I16x8GeU,
    I32x4Eq,
    I32x4Ne,
    I32x4LtS,
    I32x4LtU,
    I32x4GtS,
    I32x4GtU,
    I32x4LeS,
    I32x4LeU,
    I32x4GeS,
    I32x4GeU,
    I64x2Eq,
    I64x2Ne,
    I64x2LtS,
    I64x2GtS,
    I64x2LeS,
    I64x2GeS,
    F32x4Eq,
    F32x4Ne,
    F32x4Lt,
    F32x4Gt,
    F32x4Le,
    F32x4Ge,
    F64x2Eq,
    F64x2Ne,
    F64x2Lt,
    F64x2Gt,
    F64x2Le,
    F64x2Ge,
    V128Not,
    V128And,
    V128Andnot,
    V128Or,
    V128Xor,
    V128Bitselect,
    V128AnyTrue,
    I8x16Abs,
    I8x16Neg,
    I8x16Popcnt,
    I8x16AllTrue,
    I8x16Bitmask,
    I8x16NarrowI16x8S,
    I8x16NarrowI16x8U,
    I8x16Shl,
    I8x16ShrS,
    I8x16ShrU,
    I8x16AddSatS,
    I8x16AddSatU,
    I8x16SubSatS,
    I8x16SubSatU,
    I8x16MinU,
    I8x16MaxU,
    I8x16AvgrU,
    I16x8ExtaddPairwiseI8x16S,
    I16x8ExtaddPairwiseI8x16U,
    I16x8Abs,
    I16x8Neg,
    I16x8Q15mulrSatS,
    I16x8AllTrue,
    I16x8Bitmask,
    I16x8NarrowI32x4S,
    I16x8NarrowI32x4U,
    I16x8ExtendLowI8x16S,
    I16x8ExtendHighI8x16S,
    I16x8ExtendLowI8x16U,
    I16x8ExtendHighI8x16U,
    I16x8Shl,
    I16x8ShrS,
    I16x8ShrU,
    I16x8AddSatS,
    I16x8AddSatU,
    I16x8SubSatS,
    I16x8SubSatU,
    I16x8MinU,
    I16x8MaxU,
    I16x8AvgrU,
    I16x8ExtmulLowI8x16S,
    I16x8ExtmulHighI8x16S,
    I16x8ExtmulLowI8x16U,
    I16x8ExtmulHighI8x16U,
    I32x4ExtaddPairwiseI16x8S,
    I32x4ExtaddPairwiseI16x8U,
    I32x4Abs,
    I32x4Neg,
    I32x4AllTrue,
    I32x4Bitmask,
    I32x4ExtendLowI16x8S,
    I32x4ExtendHighI16x8S,
    I32x4ExtendLowI16x8U,
    I32x4ExtendHighI16x8U,
    I32x4Shl,
    I32x4ShrS,
    I32x4ShrU,
    I32x4MinU,
    I32x4MaxU,
    I32x4DotI16x8S,
    I32x4ExtmulLowI16x8S,
    I32x4ExtmulHighI16x8S,
    I32x4ExtmulLowI16x8U,
    I32x4ExtmulHighI16x8U,
    I64x2Abs,
    I64x2Neg,
    I64x2AllTrue,
    I64x2Bitmask,
    I64x2ExtendLowI32x4S,
    I64x2ExtendHighI32x4S,
    I64x2ExtendLowI32x4U,
    I64x2ExtendHighI32x4U,
    I64x2Shl,
    I64x2ShrS,
    I64x2ShrU,
    I64x2ExtmulLowI32x4S,
    I64x2ExtmulHighI32x4S,
    I64x2ExtmulLowI32x4U,
    I64x2ExtmulHighI32x4U,
    F32x4Ceil,
    F32x4Floor,
    F32x4Trunc,
    F32x4Nearest,
    F32x4Abs,
    F32x4Neg,
    F32x4Sqrt,
    F32x4Div,
    F32x4Pmin,
    F32x4Pmax,
    F64x2Ceil,
    F64x2Floor,
    F64x2Trunc,
    F64x2Nearest,
    F64x2Abs,
    F64x2Neg,
    F64x2Sqrt,
    F64x2Div,
    F64x2Pmin,
    F64x2Pmax,
    I32x4TruncSatF32x4S,
    I32x4TruncSatF32x4U,
    F32x4ConvertI32x4S,
    F32x4ConvertI32x4U,
    I32x4TruncSatF64x2SZero,
    I32x4TruncSatF64x2UZero,
    F64x2ConvertLowI32x4S,
    F64x2ConvertLowI32x4U,
    F32x4DemoteF64x2Zero,
    F64x2PromoteLowF32x4,
    I8x16RelaxedSwizzle,
    I32x4RelaxedTruncF32x4S,
    I32x4RelaxedTruncF32x4U,
    I32x4RelaxedTruncF64x2SZero,
    I32x4RelaxedTruncF64x2UZero,
    F32x4RelaxedMadd,
    F32x4RelaxedNmadd,
    F64x2RelaxedMadd,
    F64x2RelaxedNmadd,
    I8x16RelaxedLaneselect,
    I16x8RelaxedLaneselect,
    I32x4RelaxedLaneselect,
    I64x2RelaxedLaneselect,
    F32x4RelaxedMin,
    F32x4RelaxedMax,
    F64x2RelaxedMin,
    F64x2RelaxedMax,
    I16x8RelaxedQ15mulrS,
    I16x8RelaxedDotI8x16I7x16S,
    I32x4RelaxedDotI8x16I7x16AddS,
}

pub(super) fn encode_nullary(sink: &mut InstructionSink<'_>, op: Nullary) {
    match op {
        Nullary::I32Eqz => sink.i32_eqz(),
        Nullary::I32Eq => sink.i32_eq(),
        Nullary::I32Ne => sink.i32_ne(),
        Nullary::I32LtS => sink.i32_lt_s(),
        Nullary::I32LtU => sink.i32_lt_u(),
        Nullary::I32GtS => sink.i32_gt_s(),
        Nullary::I32GtU => sink.i32_gt_u(),
        Nullary::I32LeS => sink.i32_le_s(),
        Nullary::I32LeU => sink.i32_le_u(),
        Nullary::I32GeS => sink.i32_ge_s(),
        Nullary::I32GeU => sink.i32_ge_u(),
        Nullary::I64Eqz => sink.i64_eqz(),
        Nullary::I64Eq => sink.i64_eq(),
        Nullary::I64Ne => sink.i64_ne(),
        Nullary::I64LtS => sink.i64_lt_s(),
        Nullary::I64LtU => sink.i64_lt_u(),
        Nullary::I64GtS => sink.i64_gt_s(),
        Nullary::I64GtU => sink.i64_gt_u(),
        Nullary::I64LeS => sink.i64_le_s(),
        Nullary::I64LeU => sink.i64_le_u(),
        Nullary::I64GeS => sink.i64_ge_s(),
        Nullary::I64GeU => sink.i64_ge_u(),
        Nullary::F32Eq => sink.f32_eq(),
        Nullary::F32Ne => sink.f32_ne(),
        Nullary::F32Lt => sink.f32_lt(),
        Nullary::F32Gt => sink.f32_gt(),
        Nullary::F32Le => sink.f32_le(),
        Nullary::F32Ge => sink.f32_ge(),
        Nullary::F64Eq => sink.f64_eq(),
        Nullary::F64Ne => sink.f64_ne(),
        Nullary::F64Lt => sink.f64_lt(),
        Nullary::F64Gt => sink.f64_gt(),
        Nullary::F64Le => sink.f64_le(),
        Nullary::F64Ge => sink.f64_ge(),
        Nullary::I32Clz => sink.i32_clz(),
        Nullary::I32Ctz => sink.i32_ctz(),
        Nullary::I32Popcnt => sink.i32_popcnt(),
        Nullary::I32Add => sink.i32_add(),
        Nullary::I32Sub => sink.i32_sub(),
        Nullary::I32Mul => sink.i32_mul(),
        Nullary::I32DivS => sink.i32_div_s(),
        Nullary::I32DivU => sink.i32_div_u(),
        Nullary::I32RemS => sink.i32_rem_s(),
        Nullary::I32RemU => sink.i32_rem_u(),
        Nullary::I32And => sink.i32_and(),
        Nullary::I32Or => sink.i32_or(),
        Nullary::I32Xor => sink.i32_xor(),
        Nullary::I32Shl => sink.i32_shl(),
        Nullary::I32ShrS => sink.i32_shr_s(),
        Nullary::I32ShrU => sink.i32_shr_u(),
        Nullary::I32Rotl => sink.i32_rotl(),
        Nullary::I32Rotr => sink.i32_rotr(),
        Nullary::I64Clz => sink.i64_clz(),
        Nullary::I64Ctz => sink.i64_ctz(),
        Nullary::I64Popcnt => sink.i64_popcnt(),
        Nullary::I64Add => sink.i64_add(),
        Nullary::I64Sub => sink.i64_sub(),
        Nullary::I64Mul => sink.i64_mul(),
        Nullary::I64DivS => sink.i64_div_s(),
        Nullary::I64DivU => sink.i64_div_u(),
        Nullary::I64RemS => sink.i64_rem_s(),
        Nullary::I64RemU => sink.i64_rem_u(),
        Nullary::I64And => sink.i64_and(),
        Nullary::I64Or => sink.i64_or(),
        Nullary::I64Xor => sink.i64_xor(),
        Nullary::I64Shl => sink.i64_shl(),
        Nullary::I64ShrS => sink.i64_shr_s(),
        Nullary::I64ShrU => sink.i64_shr_u(),
        Nullary::I64Rotl => sink.i64_rotl(),
        Nullary::I64Rotr => sink.i64_rotr(),
        Nullary::F32Abs => sink.f32_abs(),
        Nullary::F32Neg => sink.f32_neg(),
        Nullary::F32Ceil => sink.f32_ceil(),
        Nullary::F32Floor => sink.f32_floor(),
        Nullary::F32Trunc => sink.f32_trunc(),
        Nullary::F32Nearest => sink.f32_nearest(),
        Nullary::F32Sqrt => sink.f32_sqrt(),
        Nullary::F32Add => sink.f32_add(),
        Nullary::F32Sub => sink.f32_sub(),
        Nullary::F32Mul => sink.f32_mul(),
        Nullary::F32Div => sink.f32_div(),
        Nullary::F32Min => sink.f32_min(),
        Nullary::F32Max => sink.f32_max(),
        Nullary::F32Copysign => sink.f32_copysign(),
        Nullary::F64Abs => sink.f64_abs(),
        Nullary::F64Neg => sink.f64_neg(),
        Nullary::F64Ceil => sink.f64_ceil(),
        Nullary::F64Floor => sink.f64_floor(),
        Nullary::F64Trunc => sink.f64_trunc(),
        Nullary::F64Nearest => sink.f64_nearest(),
        Nullary::F64Sqrt => sink.f64_sqrt(),
        Nullary::F64Add => sink.f64_add(),
        Nullary::F64Sub => sink.f64_sub(),
        Nullary::F64Mul => sink.f64_mul(),
        Nullary::F64Div => sink.f64_div(),
        Nullary::F64Min => sink.f64_min(),
        Nullary::F64Max => sink.f64_max(),
        Nullary::F64Copysign => sink.f64_copysign(),
        Nullary::I32WrapI64 => sink.i32_wrap_i64(),
        Nullary::I32TruncF32S => sink.i32_trunc_f32_s(),
        Nullary::I32TruncF32U => sink.i32_trunc_f32_u(),
        Nullary::I32TruncF64S => sink.i32_trunc_f64_s(),
        Nullary::I32TruncF64U => sink.i32_trunc_f64_u(),
        Nullary::I32TruncSatF32S => sink.i32_trunc_sat_f32_s(),
        Nullary::I32TruncSatF32U => sink.i32_trunc_sat_f32_u(),
        Nullary::I32TruncSatF64S => sink.i32_trunc_sat_f64_s(),
        Nullary::I32TruncSatF64U => sink.i32_trunc_sat_f64_u(),
        Nullary::I64TruncSatF32S => sink.i64_trunc_sat_f32_s(),
        Nullary::I64TruncSatF32U => sink.i64_trunc_sat_f32_u(),
        Nullary::I64TruncSatF64S => sink.i64_trunc_sat_f64_s(),
        Nullary::I64TruncSatF64U => sink.i64_trunc_sat_f64_u(),
        Nullary::I64ExtendI32S => sink.i64_extend_i32_s(),
        Nullary::I64ExtendI32U => sink.i64_extend_i32_u(),
        Nullary::I64TruncF32S => sink.i64_trunc_f32_s(),
        Nullary::I64TruncF32U => sink.i64_trunc_f32_u(),
        Nullary::I64TruncF64S => sink.i64_trunc_f64_s(),
        Nullary::I64TruncF64U => sink.i64_trunc_f64_u(),
        Nullary::F32ConvertI32S => sink.f32_convert_i32_s(),
        Nullary::F32ConvertI32U => sink.f32_convert_i32_u(),
        Nullary::F32ConvertI64S => sink.f32_convert_i64_s(),
        Nullary::F32ConvertI64U => sink.f32_convert_i64_u(),
        Nullary::F32DemoteF64 => sink.f32_demote_f64(),
        Nullary::F64ConvertI32S => sink.f64_convert_i32_s(),
        Nullary::F64ConvertI32U => sink.f64_convert_i32_u(),
        Nullary::F64ConvertI64S => sink.f64_convert_i64_s(),
        Nullary::F64ConvertI64U => sink.f64_convert_i64_u(),
        Nullary::F64PromoteF32 => sink.f64_promote_f32(),
        Nullary::I32ReinterpretF32 => sink.i32_reinterpret_f32(),
        Nullary::I64ReinterpretF64 => sink.i64_reinterpret_f64(),
        Nullary::F32ReinterpretI32 => sink.f32_reinterpret_i32(),
        Nullary::F64ReinterpretI64 => sink.f64_reinterpret_i64(),
        Nullary::I32EXTEND8S => sink.i32_extend8_s(),
        Nullary::I32EXTEND16S => sink.i32_extend16_s(),
        Nullary::I64EXTEND8S => sink.i64_extend8_s(),
        Nullary::I64EXTEND16S => sink.i64_extend16_s(),
        Nullary::I64EXTEND32S => sink.i64_extend32_s(),
        Nullary::I8x16Add => sink.i8x16_add(),
        Nullary::I8x16Sub => sink.i8x16_sub(),
        Nullary::I8x16MinS => sink.i8x16_min_s(),
        Nullary::I8x16MaxS => sink.i8x16_max_s(),
        Nullary::I16x8Add => sink.i16x8_add(),
        Nullary::I16x8Sub => sink.i16x8_sub(),
        Nullary::I16x8Mul => sink.i16x8_mul(),
        Nullary::I16x8MinS => sink.i16x8_min_s(),
        Nullary::I16x8MaxS => sink.i16x8_max_s(),
        Nullary::I32x4Add => sink.i32x4_add(),
        Nullary::I32x4Sub => sink.i32x4_sub(),
        Nullary::I32x4Mul => sink.i32x4_mul(),
        Nullary::I32x4MinS => sink.i32x4_min_s(),
        Nullary::I32x4MaxS => sink.i32x4_max_s(),
        Nullary::I64x2Add => sink.i64x2_add(),
        Nullary::I64x2Sub => sink.i64x2_sub(),
        Nullary::I64x2Mul => sink.i64x2_mul(),
        Nullary::F32x4Add => sink.f32x4_add(),
        Nullary::F32x4Sub => sink.f32x4_sub(),
        Nullary::F32x4Mul => sink.f32x4_mul(),
        Nullary::F32x4Min => sink.f32x4_min(),
        Nullary::F32x4Max => sink.f32x4_max(),
        Nullary::F64x2Add => sink.f64x2_add(),
        Nullary::F64x2Sub => sink.f64x2_sub(),
        Nullary::F64x2Mul => sink.f64x2_mul(),
        Nullary::F64x2Min => sink.f64x2_min(),
        Nullary::F64x2Max => sink.f64x2_max(),
        Nullary::I8x16Splat => sink.i8x16_splat(),
        Nullary::I16x8Splat => sink.i16x8_splat(),
        Nullary::I32x4Splat => sink.i32x4_splat(),
        Nullary::I64x2Splat => sink.i64x2_splat(),
        Nullary::F32x4Splat => sink.f32x4_splat(),
        Nullary::F64x2Splat => sink.f64x2_splat(),
        Nullary::I8x16Swizzle => sink.i8x16_swizzle(),
        Nullary::I8x16Eq => sink.i8x16_eq(),
        Nullary::I8x16Ne => sink.i8x16_ne(),
        Nullary::I8x16LtS => sink.i8x16_lt_s(),
        Nullary::I8x16LtU => sink.i8x16_lt_u(),
        Nullary::I8x16GtS => sink.i8x16_gt_s(),
        Nullary::I8x16GtU => sink.i8x16_gt_u(),
        Nullary::I8x16LeS => sink.i8x16_le_s(),
        Nullary::I8x16LeU => sink.i8x16_le_u(),
        Nullary::I8x16GeS => sink.i8x16_ge_s(),
        Nullary::I8x16GeU => sink.i8x16_ge_u(),
        Nullary::I16x8Eq => sink.i16x8_eq(),
        Nullary::I16x8Ne => sink.i16x8_ne(),
        Nullary::I16x8LtS => sink.i16x8_lt_s(),
        Nullary::I16x8LtU => sink.i16x8_lt_u(),
        Nullary::I16x8GtS => sink.i16x8_gt_s(),
        Nullary::I16x8GtU => sink.i16x8_gt_u(),
        Nullary::I16x8LeS => sink.i16x8_le_s(),
        Nullary::I16x8LeU => sink.i16x8_le_u(),
        Nullary::I16x8GeS => sink.i16x8_ge_s(),
        Nullary::I16x8GeU => sink.i16x8_ge_u(),
        Nullary::I32x4Eq => sink.i32x4_eq(),
        Nullary::I32x4Ne => sink.i32x4_ne(),
        Nullary::I32x4LtS => sink.i32x4_lt_s(),
        Nullary::I32x4LtU => sink.i32x4_lt_u(),
        Nullary::I32x4GtS => sink.i32x4_gt_s(),
        Nullary::I32x4GtU => sink.i32x4_gt_u(),
        Nullary::I32x4LeS => sink.i32x4_le_s(),
        Nullary::I32x4LeU => sink.i32x4_le_u(),
        Nullary::I32x4GeS => sink.i32x4_ge_s(),
        Nullary::I32x4GeU => sink.i32x4_ge_u(),
        Nullary::I64x2Eq => sink.i64x2_eq(),
        Nullary::I64x2Ne => sink.i64x2_ne(),
        Nullary::I64x2LtS => sink.i64x2_lt_s(),
        Nullary::I64x2GtS => sink.i64x2_gt_s(),
        Nullary::I64x2LeS => sink.i64x2_le_s(),
        Nullary::I64x2GeS => sink.i64x2_ge_s(),
        Nullary::F32x4Eq => sink.f32x4_eq(),
        Nullary::F32x4Ne => sink.f32x4_ne(),
        Nullary::F32x4Lt => sink.f32x4_lt(),
        Nullary::F32x4Gt => sink.f32x4_gt(),
        Nullary::F32x4Le => sink.f32x4_le(),
        Nullary::F32x4Ge => sink.f32x4_ge(),
        Nullary::F64x2Eq => sink.f64x2_eq(),
        Nullary::F64x2Ne => sink.f64x2_ne(),
        Nullary::F64x2Lt => sink.f64x2_lt(),
        Nullary::F64x2Gt => sink.f64x2_gt(),
        Nullary::F64x2Le => sink.f64x2_le(),
        Nullary::F64x2Ge => sink.f64x2_ge(),
        Nullary::V128Not => sink.v128_not(),
        Nullary::V128And => sink.v128_and(),
        Nullary::V128Andnot => sink.v128_andnot(),
        Nullary::V128Or => sink.v128_or(),
        Nullary::V128Xor => sink.v128_xor(),
        Nullary::V128Bitselect => sink.v128_bitselect(),
        Nullary::V128AnyTrue => sink.v128_any_true(),
        Nullary::I8x16Abs => sink.i8x16_abs(),
        Nullary::I8x16Neg => sink.i8x16_neg(),
        Nullary::I8x16Popcnt => sink.i8x16_popcnt(),
        Nullary::I8x16AllTrue => sink.i8x16_all_true(),
        Nullary::I8x16Bitmask => sink.i8x16_bitmask(),
        Nullary::I8x16NarrowI16x8S => sink.i8x16_narrow_i16x8_s(),
        Nullary::I8x16NarrowI16x8U => sink.i8x16_narrow_i16x8_u(),
        Nullary::I8x16Shl => sink.i8x16_shl(),
        Nullary::I8x16ShrS => sink.i8x16_shr_s(),
        Nullary::I8x16ShrU => sink.i8x16_shr_u(),
        Nullary::I8x16AddSatS => sink.i8x16_add_sat_s(),
        Nullary::I8x16AddSatU => sink.i8x16_add_sat_u(),
        Nullary::I8x16SubSatS => sink.i8x16_sub_sat_s(),
        Nullary::I8x16SubSatU => sink.i8x16_sub_sat_u(),
        Nullary::I8x16MinU => sink.i8x16_min_u(),
        Nullary::I8x16MaxU => sink.i8x16_max_u(),
        Nullary::I8x16AvgrU => sink.i8x16_avgr_u(),
        Nullary::I16x8ExtaddPairwiseI8x16S => sink.i16x8_extadd_pairwise_i8x16_s(),
        Nullary::I16x8ExtaddPairwiseI8x16U => sink.i16x8_extadd_pairwise_i8x16_u(),
        Nullary::I16x8Abs => sink.i16x8_abs(),
        Nullary::I16x8Neg => sink.i16x8_neg(),
        Nullary::I16x8Q15mulrSatS => sink.i16x8_q15mulr_sat_s(),
        Nullary::I16x8AllTrue => sink.i16x8_all_true(),
        Nullary::I16x8Bitmask => sink.i16x8_bitmask(),
        Nullary::I16x8NarrowI32x4S => sink.i16x8_narrow_i32x4_s(),
        Nullary::I16x8NarrowI32x4U => sink.i16x8_narrow_i32x4_u(),
        Nullary::I16x8ExtendLowI8x16S => sink.i16x8_extend_low_i8x16_s(),
        Nullary::I16x8ExtendHighI8x16S => sink.i16x8_extend_high_i8x16_s(),
        Nullary::I16x8ExtendLowI8x16U => sink.i16x8_extend_low_i8x16_u(),
        Nullary::I16x8ExtendHighI8x16U => sink.i16x8_extend_high_i8x16_u(),
        Nullary::I16x8Shl => sink.i16x8_shl(),
        Nullary::I16x8ShrS => sink.i16x8_shr_s(),
        Nullary::I16x8ShrU => sink.i16x8_shr_u(),
        Nullary::I16x8AddSatS => sink.i16x8_add_sat_s(),
        Nullary::I16x8AddSatU => sink.i16x8_add_sat_u(),
        Nullary::I16x8SubSatS => sink.i16x8_sub_sat_s(),
        Nullary::I16x8SubSatU => sink.i16x8_sub_sat_u(),
        Nullary::I16x8MinU => sink.i16x8_min_u(),
        Nullary::I16x8MaxU => sink.i16x8_max_u(),
        Nullary::I16x8AvgrU => sink.i16x8_avgr_u(),
        Nullary::I16x8ExtmulLowI8x16S => sink.i16x8_extmul_low_i8x16_s(),
        Nullary::I16x8ExtmulHighI8x16S => sink.i16x8_extmul_high_i8x16_s(),
        Nullary::I16x8ExtmulLowI8x16U => sink.i16x8_extmul_low_i8x16_u(),
        Nullary::I16x8ExtmulHighI8x16U => sink.i16x8_extmul_high_i8x16_u(),
        Nullary::I32x4ExtaddPairwiseI16x8S => sink.i32x4_extadd_pairwise_i16x8_s(),
        Nullary::I32x4ExtaddPairwiseI16x8U => sink.i32x4_extadd_pairwise_i16x8_u(),
        Nullary::I32x4Abs => sink.i32x4_abs(),
        Nullary::I32x4Neg => sink.i32x4_neg(),
        Nullary::I32x4AllTrue => sink.i32x4_all_true(),
        Nullary::I32x4Bitmask => sink.i32x4_bitmask(),
        Nullary::I32x4ExtendLowI16x8S => sink.i32x4_extend_low_i16x8_s(),
        Nullary::I32x4ExtendHighI16x8S => sink.i32x4_extend_high_i16x8_s(),
        Nullary::I32x4ExtendLowI16x8U => sink.i32x4_extend_low_i16x8_u(),
        Nullary::I32x4ExtendHighI16x8U => sink.i32x4_extend_high_i16x8_u(),
        Nullary::I32x4Shl => sink.i32x4_shl(),
        Nullary::I32x4ShrS => sink.i32x4_shr_s(),
        Nullary::I32x4ShrU => sink.i32x4_shr_u(),
        Nullary::I32x4MinU => sink.i32x4_min_u(),
        Nullary::I32x4MaxU => sink.i32x4_max_u(),
        Nullary::I32x4DotI16x8S => sink.i32x4_dot_i16x8_s(),
        Nullary::I32x4ExtmulLowI16x8S => sink.i32x4_extmul_low_i16x8_s(),
        Nullary::I32x4ExtmulHighI16x8S => sink.i32x4_extmul_high_i16x8_s(),
        Nullary::I32x4ExtmulLowI16x8U => sink.i32x4_extmul_low_i16x8_u(),
        Nullary::I32x4ExtmulHighI16x8U => sink.i32x4_extmul_high_i16x8_u(),
        Nullary::I64x2Abs => sink.i64x2_abs(),
        Nullary::I64x2Neg => sink.i64x2_neg(),
        Nullary::I64x2AllTrue => sink.i64x2_all_true(),
        Nullary::I64x2Bitmask => sink.i64x2_bitmask(),
        Nullary::I64x2ExtendLowI32x4S => sink.i64x2_extend_low_i32x4_s(),
        Nullary::I64x2ExtendHighI32x4S => sink.i64x2_extend_high_i32x4_s(),
        Nullary::I64x2ExtendLowI32x4U => sink.i64x2_extend_low_i32x4_u(),
        Nullary::I64x2ExtendHighI32x4U => sink.i64x2_extend_high_i32x4_u(),
        Nullary::I64x2Shl => sink.i64x2_shl(),
        Nullary::I64x2ShrS => sink.i64x2_shr_s(),
        Nullary::I64x2ShrU => sink.i64x2_shr_u(),
        Nullary::I64x2ExtmulLowI32x4S => sink.i64x2_extmul_low_i32x4_s(),
        Nullary::I64x2ExtmulHighI32x4S => sink.i64x2_extmul_high_i32x4_s(),
        Nullary::I64x2ExtmulLowI32x4U => sink.i64x2_extmul_low_i32x4_u(),
        Nullary::I64x2ExtmulHighI32x4U => sink.i64x2_extmul_high_i32x4_u(),
        Nullary::F32x4Ceil => sink.f32x4_ceil(),
        Nullary::F32x4Floor => sink.f32x4_floor(),
        Nullary::F32x4Trunc => sink.f32x4_trunc(),
        Nullary::F32x4Nearest => sink.f32x4_nearest(),
        Nullary::F32x4Abs => sink.f32x4_abs(),
        Nullary::F32x4Neg => sink.f32x4_neg(),
        Nullary::F32x4Sqrt => sink.f32x4_sqrt(),
        Nullary::F32x4Div => sink.f32x4_div(),
        Nullary::F32x4Pmin => sink.f32x4_pmin(),
        Nullary::F32x4Pmax => sink.f32x4_pmax(),
        Nullary::F64x2Ceil => sink.f64x2_ceil(),
        Nullary::F64x2Floor => sink.f64x2_floor(),
        Nullary::F64x2Trunc => sink.f64x2_trunc(),
        Nullary::F64x2Nearest => sink.f64x2_nearest(),
        Nullary::F64x2Abs => sink.f64x2_abs(),
        Nullary::F64x2Neg => sink.f64x2_neg(),
        Nullary::F64x2Sqrt => sink.f64x2_sqrt(),
        Nullary::F64x2Div => sink.f64x2_div(),
        Nullary::F64x2Pmin => sink.f64x2_pmin(),
        Nullary::F64x2Pmax => sink.f64x2_pmax(),
        Nullary::I32x4TruncSatF32x4S => sink.i32x4_trunc_sat_f32x4_s(),
        Nullary::I32x4TruncSatF32x4U => sink.i32x4_trunc_sat_f32x4_u(),
        Nullary::F32x4ConvertI32x4S => sink.f32x4_convert_i32x4_s(),
        Nullary::F32x4ConvertI32x4U => sink.f32x4_convert_i32x4_u(),
        Nullary::I32x4TruncSatF64x2SZero => sink.i32x4_trunc_sat_f64x2_s_zero(),
        Nullary::I32x4TruncSatF64x2UZero => sink.i32x4_trunc_sat_f64x2_u_zero(),
        Nullary::F64x2ConvertLowI32x4S => sink.f64x2_convert_low_i32x4_s(),
        Nullary::F64x2ConvertLowI32x4U => sink.f64x2_convert_low_i32x4_u(),
        Nullary::F32x4DemoteF64x2Zero => sink.f32x4_demote_f64x2_zero(),
        Nullary::F64x2PromoteLowF32x4 => sink.f64x2_promote_low_f32x4(),
        Nullary::I8x16RelaxedSwizzle => sink.i8x16_relaxed_swizzle(),
        Nullary::I32x4RelaxedTruncF32x4S => sink.i32x4_relaxed_trunc_f32x4_s(),
        Nullary::I32x4RelaxedTruncF32x4U => sink.i32x4_relaxed_trunc_f32x4_u(),
        Nullary::I32x4RelaxedTruncF64x2SZero => sink.i32x4_relaxed_trunc_f64x2_s_zero(),
        Nullary::I32x4RelaxedTruncF64x2UZero => sink.i32x4_relaxed_trunc_f64x2_u_zero(),
        Nullary::F32x4RelaxedMadd => sink.f32x4_relaxed_madd(),
        Nullary::F32x4RelaxedNmadd => sink.f32x4_relaxed_nmadd(),
        Nullary::F64x2RelaxedMadd => sink.f64x2_relaxed_madd(),
        Nullary::F64x2RelaxedNmadd => sink.f64x2_relaxed_nmadd(),
        Nullary::I8x16RelaxedLaneselect => sink.i8x16_relaxed_laneselect(),
        Nullary::I16x8RelaxedLaneselect => sink.i16x8_relaxed_laneselect(),
        Nullary::I32x4RelaxedLaneselect => sink.i32x4_relaxed_laneselect(),
        Nullary::I64x2RelaxedLaneselect => sink.i64x2_relaxed_laneselect(),
        Nullary::F32x4RelaxedMin => sink.f32x4_relaxed_min(),
        Nullary::F32x4RelaxedMax => sink.f32x4_relaxed_max(),
        Nullary::F64x2RelaxedMin => sink.f64x2_relaxed_min(),
        Nullary::F64x2RelaxedMax => sink.f64x2_relaxed_max(),
        Nullary::I16x8RelaxedQ15mulrS => sink.i16x8_relaxed_q15mulr_s(),
        Nullary::I16x8RelaxedDotI8x16I7x16S => sink.i16x8_relaxed_dot_i8x16_i7x16_s(),
        Nullary::I32x4RelaxedDotI8x16I7x16AddS => sink.i32x4_relaxed_dot_i8x16_i7x16_add_s(),
    };
}

use super::func::{FuncBuilder, Op};

#[allow(dead_code)]
impl FuncBuilder {
    pub(crate) fn i32_eqz(&mut self) {
        self.push(Op::Nullary(Nullary::I32Eqz));
    }

    pub(crate) fn i32_eq(&mut self) {
        self.push(Op::Nullary(Nullary::I32Eq));
    }

    pub(crate) fn i32_ne(&mut self) {
        self.push(Op::Nullary(Nullary::I32Ne));
    }

    pub(crate) fn i32_lt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32LtS));
    }

    pub(crate) fn i32_lt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32LtU));
    }

    pub(crate) fn i32_gt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32GtS));
    }

    pub(crate) fn i32_gt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32GtU));
    }

    pub(crate) fn i32_le_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32LeS));
    }

    pub(crate) fn i32_le_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32LeU));
    }

    pub(crate) fn i32_ge_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32GeS));
    }

    pub(crate) fn i32_ge_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32GeU));
    }

    pub(crate) fn i64_eqz(&mut self) {
        self.push(Op::Nullary(Nullary::I64Eqz));
    }

    pub(crate) fn i64_eq(&mut self) {
        self.push(Op::Nullary(Nullary::I64Eq));
    }

    pub(crate) fn i64_ne(&mut self) {
        self.push(Op::Nullary(Nullary::I64Ne));
    }

    pub(crate) fn i64_lt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64LtS));
    }

    pub(crate) fn i64_lt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64LtU));
    }

    pub(crate) fn i64_gt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64GtS));
    }

    pub(crate) fn i64_gt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64GtU));
    }

    pub(crate) fn i64_le_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64LeS));
    }

    pub(crate) fn i64_le_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64LeU));
    }

    pub(crate) fn i64_ge_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64GeS));
    }

    pub(crate) fn i64_ge_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64GeU));
    }

    pub(crate) fn f32_eq(&mut self) {
        self.push(Op::Nullary(Nullary::F32Eq));
    }

    pub(crate) fn f32_ne(&mut self) {
        self.push(Op::Nullary(Nullary::F32Ne));
    }

    pub(crate) fn f32_lt(&mut self) {
        self.push(Op::Nullary(Nullary::F32Lt));
    }

    pub(crate) fn f32_gt(&mut self) {
        self.push(Op::Nullary(Nullary::F32Gt));
    }

    pub(crate) fn f32_le(&mut self) {
        self.push(Op::Nullary(Nullary::F32Le));
    }

    pub(crate) fn f32_ge(&mut self) {
        self.push(Op::Nullary(Nullary::F32Ge));
    }

    pub(crate) fn f64_eq(&mut self) {
        self.push(Op::Nullary(Nullary::F64Eq));
    }

    pub(crate) fn f64_ne(&mut self) {
        self.push(Op::Nullary(Nullary::F64Ne));
    }

    pub(crate) fn f64_lt(&mut self) {
        self.push(Op::Nullary(Nullary::F64Lt));
    }

    pub(crate) fn f64_gt(&mut self) {
        self.push(Op::Nullary(Nullary::F64Gt));
    }

    pub(crate) fn f64_le(&mut self) {
        self.push(Op::Nullary(Nullary::F64Le));
    }

    pub(crate) fn f64_ge(&mut self) {
        self.push(Op::Nullary(Nullary::F64Ge));
    }

    pub(crate) fn i32_clz(&mut self) {
        self.push(Op::Nullary(Nullary::I32Clz));
    }

    pub(crate) fn i32_ctz(&mut self) {
        self.push(Op::Nullary(Nullary::I32Ctz));
    }

    pub(crate) fn i32_popcnt(&mut self) {
        self.push(Op::Nullary(Nullary::I32Popcnt));
    }

    pub(crate) fn i32_add(&mut self) {
        self.push(Op::Nullary(Nullary::I32Add));
    }

    pub(crate) fn i32_sub(&mut self) {
        self.push(Op::Nullary(Nullary::I32Sub));
    }

    pub(crate) fn i32_mul(&mut self) {
        self.push(Op::Nullary(Nullary::I32Mul));
    }

    pub(crate) fn i32_div_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32DivS));
    }

    pub(crate) fn i32_div_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32DivU));
    }

    pub(crate) fn i32_rem_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32RemS));
    }

    pub(crate) fn i32_rem_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32RemU));
    }

    pub(crate) fn i32_and(&mut self) {
        self.push(Op::Nullary(Nullary::I32And));
    }

    pub(crate) fn i32_or(&mut self) {
        self.push(Op::Nullary(Nullary::I32Or));
    }

    pub(crate) fn i32_xor(&mut self) {
        self.push(Op::Nullary(Nullary::I32Xor));
    }

    pub(crate) fn i32_shl(&mut self) {
        self.push(Op::Nullary(Nullary::I32Shl));
    }

    pub(crate) fn i32_shr_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32ShrS));
    }

    pub(crate) fn i32_shr_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32ShrU));
    }

    pub(crate) fn i32_rotl(&mut self) {
        self.push(Op::Nullary(Nullary::I32Rotl));
    }

    pub(crate) fn i32_rotr(&mut self) {
        self.push(Op::Nullary(Nullary::I32Rotr));
    }

    pub(crate) fn i64_clz(&mut self) {
        self.push(Op::Nullary(Nullary::I64Clz));
    }

    pub(crate) fn i64_ctz(&mut self) {
        self.push(Op::Nullary(Nullary::I64Ctz));
    }

    pub(crate) fn i64_popcnt(&mut self) {
        self.push(Op::Nullary(Nullary::I64Popcnt));
    }

    pub(crate) fn i64_add(&mut self) {
        self.push(Op::Nullary(Nullary::I64Add));
    }

    pub(crate) fn i64_sub(&mut self) {
        self.push(Op::Nullary(Nullary::I64Sub));
    }

    pub(crate) fn i64_mul(&mut self) {
        self.push(Op::Nullary(Nullary::I64Mul));
    }

    pub(crate) fn i64_div_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64DivS));
    }

    pub(crate) fn i64_div_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64DivU));
    }

    pub(crate) fn i64_rem_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64RemS));
    }

    pub(crate) fn i64_rem_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64RemU));
    }

    pub(crate) fn i64_and(&mut self) {
        self.push(Op::Nullary(Nullary::I64And));
    }

    pub(crate) fn i64_or(&mut self) {
        self.push(Op::Nullary(Nullary::I64Or));
    }

    pub(crate) fn i64_xor(&mut self) {
        self.push(Op::Nullary(Nullary::I64Xor));
    }

    pub(crate) fn i64_shl(&mut self) {
        self.push(Op::Nullary(Nullary::I64Shl));
    }

    pub(crate) fn i64_shr_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64ShrS));
    }

    pub(crate) fn i64_shr_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64ShrU));
    }

    pub(crate) fn i64_rotl(&mut self) {
        self.push(Op::Nullary(Nullary::I64Rotl));
    }

    pub(crate) fn i64_rotr(&mut self) {
        self.push(Op::Nullary(Nullary::I64Rotr));
    }

    pub(crate) fn f32_abs(&mut self) {
        self.push(Op::Nullary(Nullary::F32Abs));
    }

    pub(crate) fn f32_neg(&mut self) {
        self.push(Op::Nullary(Nullary::F32Neg));
    }

    pub(crate) fn f32_ceil(&mut self) {
        self.push(Op::Nullary(Nullary::F32Ceil));
    }

    pub(crate) fn f32_floor(&mut self) {
        self.push(Op::Nullary(Nullary::F32Floor));
    }

    pub(crate) fn f32_trunc(&mut self) {
        self.push(Op::Nullary(Nullary::F32Trunc));
    }

    pub(crate) fn f32_nearest(&mut self) {
        self.push(Op::Nullary(Nullary::F32Nearest));
    }

    pub(crate) fn f32_sqrt(&mut self) {
        self.push(Op::Nullary(Nullary::F32Sqrt));
    }

    pub(crate) fn f32_add(&mut self) {
        self.push(Op::Nullary(Nullary::F32Add));
    }

    pub(crate) fn f32_sub(&mut self) {
        self.push(Op::Nullary(Nullary::F32Sub));
    }

    pub(crate) fn f32_mul(&mut self) {
        self.push(Op::Nullary(Nullary::F32Mul));
    }

    pub(crate) fn f32_div(&mut self) {
        self.push(Op::Nullary(Nullary::F32Div));
    }

    pub(crate) fn f32_min(&mut self) {
        self.push(Op::Nullary(Nullary::F32Min));
    }

    pub(crate) fn f32_max(&mut self) {
        self.push(Op::Nullary(Nullary::F32Max));
    }

    pub(crate) fn f32_copysign(&mut self) {
        self.push(Op::Nullary(Nullary::F32Copysign));
    }

    pub(crate) fn f64_abs(&mut self) {
        self.push(Op::Nullary(Nullary::F64Abs));
    }

    pub(crate) fn f64_neg(&mut self) {
        self.push(Op::Nullary(Nullary::F64Neg));
    }

    pub(crate) fn f64_ceil(&mut self) {
        self.push(Op::Nullary(Nullary::F64Ceil));
    }

    pub(crate) fn f64_floor(&mut self) {
        self.push(Op::Nullary(Nullary::F64Floor));
    }

    pub(crate) fn f64_trunc(&mut self) {
        self.push(Op::Nullary(Nullary::F64Trunc));
    }

    pub(crate) fn f64_nearest(&mut self) {
        self.push(Op::Nullary(Nullary::F64Nearest));
    }

    pub(crate) fn f64_sqrt(&mut self) {
        self.push(Op::Nullary(Nullary::F64Sqrt));
    }

    pub(crate) fn f64_add(&mut self) {
        self.push(Op::Nullary(Nullary::F64Add));
    }

    pub(crate) fn f64_sub(&mut self) {
        self.push(Op::Nullary(Nullary::F64Sub));
    }

    pub(crate) fn f64_mul(&mut self) {
        self.push(Op::Nullary(Nullary::F64Mul));
    }

    pub(crate) fn f64_div(&mut self) {
        self.push(Op::Nullary(Nullary::F64Div));
    }

    pub(crate) fn f64_min(&mut self) {
        self.push(Op::Nullary(Nullary::F64Min));
    }

    pub(crate) fn f64_max(&mut self) {
        self.push(Op::Nullary(Nullary::F64Max));
    }

    pub(crate) fn f64_copysign(&mut self) {
        self.push(Op::Nullary(Nullary::F64Copysign));
    }

    pub(crate) fn i32_wrap_i64(&mut self) {
        self.push(Op::Nullary(Nullary::I32WrapI64));
    }

    pub(crate) fn i32_trunc_f32_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32TruncF32S));
    }

    pub(crate) fn i32_trunc_f32_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32TruncF32U));
    }

    pub(crate) fn i32_trunc_f64_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32TruncF64S));
    }

    pub(crate) fn i32_trunc_f64_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32TruncF64U));
    }

    pub(crate) fn i32_trunc_sat_f32_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32TruncSatF32S));
    }

    pub(crate) fn i32_trunc_sat_f32_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32TruncSatF32U));
    }

    pub(crate) fn i32_trunc_sat_f64_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32TruncSatF64S));
    }

    pub(crate) fn i32_trunc_sat_f64_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32TruncSatF64U));
    }

    pub(crate) fn i64_trunc_sat_f32_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64TruncSatF32S));
    }

    pub(crate) fn i64_trunc_sat_f32_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64TruncSatF32U));
    }

    pub(crate) fn i64_trunc_sat_f64_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64TruncSatF64S));
    }

    pub(crate) fn i64_trunc_sat_f64_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64TruncSatF64U));
    }

    pub(crate) fn i64_extend_i32_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64ExtendI32S));
    }

    pub(crate) fn i64_extend_i32_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64ExtendI32U));
    }

    pub(crate) fn i64_trunc_f32_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64TruncF32S));
    }

    pub(crate) fn i64_trunc_f32_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64TruncF32U));
    }

    pub(crate) fn i64_trunc_f64_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64TruncF64S));
    }

    pub(crate) fn i64_trunc_f64_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64TruncF64U));
    }

    pub(crate) fn f32_convert_i32_s(&mut self) {
        self.push(Op::Nullary(Nullary::F32ConvertI32S));
    }

    pub(crate) fn f32_convert_i32_u(&mut self) {
        self.push(Op::Nullary(Nullary::F32ConvertI32U));
    }

    pub(crate) fn f32_convert_i64_s(&mut self) {
        self.push(Op::Nullary(Nullary::F32ConvertI64S));
    }

    pub(crate) fn f32_convert_i64_u(&mut self) {
        self.push(Op::Nullary(Nullary::F32ConvertI64U));
    }

    pub(crate) fn f32_demote_f64(&mut self) {
        self.push(Op::Nullary(Nullary::F32DemoteF64));
    }

    pub(crate) fn f64_convert_i32_s(&mut self) {
        self.push(Op::Nullary(Nullary::F64ConvertI32S));
    }

    pub(crate) fn f64_convert_i32_u(&mut self) {
        self.push(Op::Nullary(Nullary::F64ConvertI32U));
    }

    pub(crate) fn f64_convert_i64_s(&mut self) {
        self.push(Op::Nullary(Nullary::F64ConvertI64S));
    }

    pub(crate) fn f64_convert_i64_u(&mut self) {
        self.push(Op::Nullary(Nullary::F64ConvertI64U));
    }

    pub(crate) fn f64_promote_f32(&mut self) {
        self.push(Op::Nullary(Nullary::F64PromoteF32));
    }

    pub(crate) fn i32_reinterpret_f32(&mut self) {
        self.push(Op::Nullary(Nullary::I32ReinterpretF32));
    }

    pub(crate) fn i64_reinterpret_f64(&mut self) {
        self.push(Op::Nullary(Nullary::I64ReinterpretF64));
    }

    pub(crate) fn f32_reinterpret_i32(&mut self) {
        self.push(Op::Nullary(Nullary::F32ReinterpretI32));
    }

    pub(crate) fn f64_reinterpret_i64(&mut self) {
        self.push(Op::Nullary(Nullary::F64ReinterpretI64));
    }

    pub(crate) fn i32_extend8_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32EXTEND8S));
    }

    pub(crate) fn i32_extend16_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32EXTEND16S));
    }

    pub(crate) fn i64_extend8_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64EXTEND8S));
    }

    pub(crate) fn i64_extend16_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64EXTEND16S));
    }

    pub(crate) fn i64_extend32_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64EXTEND32S));
    }

    pub(crate) fn i8x16_add(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Add));
    }

    pub(crate) fn i8x16_sub(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Sub));
    }

    pub(crate) fn i8x16_min_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16MinS));
    }

    pub(crate) fn i8x16_max_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16MaxS));
    }

    pub(crate) fn i16x8_add(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Add));
    }

    pub(crate) fn i16x8_sub(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Sub));
    }

    pub(crate) fn i16x8_mul(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Mul));
    }

    pub(crate) fn i16x8_min_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8MinS));
    }

    pub(crate) fn i16x8_max_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8MaxS));
    }

    pub(crate) fn i32x4_add(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Add));
    }

    pub(crate) fn i32x4_sub(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Sub));
    }

    pub(crate) fn i32x4_mul(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Mul));
    }

    pub(crate) fn i32x4_min_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4MinS));
    }

    pub(crate) fn i32x4_max_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4MaxS));
    }

    pub(crate) fn i64x2_add(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Add));
    }

    pub(crate) fn i64x2_sub(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Sub));
    }

    pub(crate) fn i64x2_mul(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Mul));
    }

    pub(crate) fn f32x4_add(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Add));
    }

    pub(crate) fn f32x4_sub(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Sub));
    }

    pub(crate) fn f32x4_mul(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Mul));
    }

    pub(crate) fn f32x4_min(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Min));
    }

    pub(crate) fn f32x4_max(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Max));
    }

    pub(crate) fn f64x2_add(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Add));
    }

    pub(crate) fn f64x2_sub(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Sub));
    }

    pub(crate) fn f64x2_mul(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Mul));
    }

    pub(crate) fn f64x2_min(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Min));
    }

    pub(crate) fn f64x2_max(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Max));
    }

    pub(crate) fn i8x16_splat(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Splat));
    }

    pub(crate) fn i16x8_splat(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Splat));
    }

    pub(crate) fn i32x4_splat(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Splat));
    }

    pub(crate) fn i64x2_splat(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Splat));
    }

    pub(crate) fn f32x4_splat(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Splat));
    }

    pub(crate) fn f64x2_splat(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Splat));
    }

    pub(crate) fn i8x16_swizzle(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Swizzle));
    }

    pub(crate) fn i8x16_eq(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Eq));
    }

    pub(crate) fn i8x16_ne(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Ne));
    }

    pub(crate) fn i8x16_lt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16LtS));
    }

    pub(crate) fn i8x16_lt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16LtU));
    }

    pub(crate) fn i8x16_gt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16GtS));
    }

    pub(crate) fn i8x16_gt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16GtU));
    }

    pub(crate) fn i8x16_le_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16LeS));
    }

    pub(crate) fn i8x16_le_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16LeU));
    }

    pub(crate) fn i8x16_ge_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16GeS));
    }

    pub(crate) fn i8x16_ge_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16GeU));
    }

    pub(crate) fn i16x8_eq(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Eq));
    }

    pub(crate) fn i16x8_ne(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Ne));
    }

    pub(crate) fn i16x8_lt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8LtS));
    }

    pub(crate) fn i16x8_lt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8LtU));
    }

    pub(crate) fn i16x8_gt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8GtS));
    }

    pub(crate) fn i16x8_gt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8GtU));
    }

    pub(crate) fn i16x8_le_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8LeS));
    }

    pub(crate) fn i16x8_le_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8LeU));
    }

    pub(crate) fn i16x8_ge_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8GeS));
    }

    pub(crate) fn i16x8_ge_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8GeU));
    }

    pub(crate) fn i32x4_eq(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Eq));
    }

    pub(crate) fn i32x4_ne(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Ne));
    }

    pub(crate) fn i32x4_lt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4LtS));
    }

    pub(crate) fn i32x4_lt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4LtU));
    }

    pub(crate) fn i32x4_gt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4GtS));
    }

    pub(crate) fn i32x4_gt_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4GtU));
    }

    pub(crate) fn i32x4_le_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4LeS));
    }

    pub(crate) fn i32x4_le_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4LeU));
    }

    pub(crate) fn i32x4_ge_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4GeS));
    }

    pub(crate) fn i32x4_ge_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4GeU));
    }

    pub(crate) fn i64x2_eq(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Eq));
    }

    pub(crate) fn i64x2_ne(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Ne));
    }

    pub(crate) fn i64x2_lt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2LtS));
    }

    pub(crate) fn i64x2_gt_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2GtS));
    }

    pub(crate) fn i64x2_le_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2LeS));
    }

    pub(crate) fn i64x2_ge_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2GeS));
    }

    pub(crate) fn f32x4_eq(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Eq));
    }

    pub(crate) fn f32x4_ne(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Ne));
    }

    pub(crate) fn f32x4_lt(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Lt));
    }

    pub(crate) fn f32x4_gt(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Gt));
    }

    pub(crate) fn f32x4_le(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Le));
    }

    pub(crate) fn f32x4_ge(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Ge));
    }

    pub(crate) fn f64x2_eq(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Eq));
    }

    pub(crate) fn f64x2_ne(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Ne));
    }

    pub(crate) fn f64x2_lt(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Lt));
    }

    pub(crate) fn f64x2_gt(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Gt));
    }

    pub(crate) fn f64x2_le(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Le));
    }

    pub(crate) fn f64x2_ge(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Ge));
    }

    pub(crate) fn v128_not(&mut self) {
        self.push(Op::Nullary(Nullary::V128Not));
    }

    pub(crate) fn v128_and(&mut self) {
        self.push(Op::Nullary(Nullary::V128And));
    }

    pub(crate) fn v128_andnot(&mut self) {
        self.push(Op::Nullary(Nullary::V128Andnot));
    }

    pub(crate) fn v128_or(&mut self) {
        self.push(Op::Nullary(Nullary::V128Or));
    }

    pub(crate) fn v128_xor(&mut self) {
        self.push(Op::Nullary(Nullary::V128Xor));
    }

    pub(crate) fn v128_bitselect(&mut self) {
        self.push(Op::Nullary(Nullary::V128Bitselect));
    }

    pub(crate) fn v128_any_true(&mut self) {
        self.push(Op::Nullary(Nullary::V128AnyTrue));
    }

    pub(crate) fn i8x16_abs(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Abs));
    }

    pub(crate) fn i8x16_neg(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Neg));
    }

    pub(crate) fn i8x16_popcnt(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Popcnt));
    }

    pub(crate) fn i8x16_all_true(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16AllTrue));
    }

    pub(crate) fn i8x16_bitmask(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Bitmask));
    }

    pub(crate) fn i8x16_narrow_i16x8_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16NarrowI16x8S));
    }

    pub(crate) fn i8x16_narrow_i16x8_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16NarrowI16x8U));
    }

    pub(crate) fn i8x16_shl(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16Shl));
    }

    pub(crate) fn i8x16_shr_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16ShrS));
    }

    pub(crate) fn i8x16_shr_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16ShrU));
    }

    pub(crate) fn i8x16_add_sat_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16AddSatS));
    }

    pub(crate) fn i8x16_add_sat_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16AddSatU));
    }

    pub(crate) fn i8x16_sub_sat_s(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16SubSatS));
    }

    pub(crate) fn i8x16_sub_sat_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16SubSatU));
    }

    pub(crate) fn i8x16_min_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16MinU));
    }

    pub(crate) fn i8x16_max_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16MaxU));
    }

    pub(crate) fn i8x16_avgr_u(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16AvgrU));
    }

    pub(crate) fn i16x8_extadd_pairwise_i8x16_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtaddPairwiseI8x16S));
    }

    pub(crate) fn i16x8_extadd_pairwise_i8x16_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtaddPairwiseI8x16U));
    }

    pub(crate) fn i16x8_abs(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Abs));
    }

    pub(crate) fn i16x8_neg(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Neg));
    }

    pub(crate) fn i16x8_q15mulr_sat_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Q15mulrSatS));
    }

    pub(crate) fn i16x8_all_true(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8AllTrue));
    }

    pub(crate) fn i16x8_bitmask(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Bitmask));
    }

    pub(crate) fn i16x8_narrow_i32x4_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8NarrowI32x4S));
    }

    pub(crate) fn i16x8_narrow_i32x4_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8NarrowI32x4U));
    }

    pub(crate) fn i16x8_extend_low_i8x16_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtendLowI8x16S));
    }

    pub(crate) fn i16x8_extend_high_i8x16_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtendHighI8x16S));
    }

    pub(crate) fn i16x8_extend_low_i8x16_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtendLowI8x16U));
    }

    pub(crate) fn i16x8_extend_high_i8x16_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtendHighI8x16U));
    }

    pub(crate) fn i16x8_shl(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8Shl));
    }

    pub(crate) fn i16x8_shr_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ShrS));
    }

    pub(crate) fn i16x8_shr_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ShrU));
    }

    pub(crate) fn i16x8_add_sat_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8AddSatS));
    }

    pub(crate) fn i16x8_add_sat_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8AddSatU));
    }

    pub(crate) fn i16x8_sub_sat_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8SubSatS));
    }

    pub(crate) fn i16x8_sub_sat_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8SubSatU));
    }

    pub(crate) fn i16x8_min_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8MinU));
    }

    pub(crate) fn i16x8_max_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8MaxU));
    }

    pub(crate) fn i16x8_avgr_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8AvgrU));
    }

    pub(crate) fn i16x8_extmul_low_i8x16_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtmulLowI8x16S));
    }

    pub(crate) fn i16x8_extmul_high_i8x16_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtmulHighI8x16S));
    }

    pub(crate) fn i16x8_extmul_low_i8x16_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtmulLowI8x16U));
    }

    pub(crate) fn i16x8_extmul_high_i8x16_u(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8ExtmulHighI8x16U));
    }

    pub(crate) fn i32x4_extadd_pairwise_i16x8_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtaddPairwiseI16x8S));
    }

    pub(crate) fn i32x4_extadd_pairwise_i16x8_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtaddPairwiseI16x8U));
    }

    pub(crate) fn i32x4_abs(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Abs));
    }

    pub(crate) fn i32x4_neg(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Neg));
    }

    pub(crate) fn i32x4_all_true(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4AllTrue));
    }

    pub(crate) fn i32x4_bitmask(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Bitmask));
    }

    pub(crate) fn i32x4_extend_low_i16x8_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtendLowI16x8S));
    }

    pub(crate) fn i32x4_extend_high_i16x8_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtendHighI16x8S));
    }

    pub(crate) fn i32x4_extend_low_i16x8_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtendLowI16x8U));
    }

    pub(crate) fn i32x4_extend_high_i16x8_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtendHighI16x8U));
    }

    pub(crate) fn i32x4_shl(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4Shl));
    }

    pub(crate) fn i32x4_shr_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ShrS));
    }

    pub(crate) fn i32x4_shr_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ShrU));
    }

    pub(crate) fn i32x4_min_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4MinU));
    }

    pub(crate) fn i32x4_max_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4MaxU));
    }

    pub(crate) fn i32x4_dot_i16x8_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4DotI16x8S));
    }

    pub(crate) fn i32x4_extmul_low_i16x8_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtmulLowI16x8S));
    }

    pub(crate) fn i32x4_extmul_high_i16x8_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtmulHighI16x8S));
    }

    pub(crate) fn i32x4_extmul_low_i16x8_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtmulLowI16x8U));
    }

    pub(crate) fn i32x4_extmul_high_i16x8_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4ExtmulHighI16x8U));
    }

    pub(crate) fn i64x2_abs(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Abs));
    }

    pub(crate) fn i64x2_neg(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Neg));
    }

    pub(crate) fn i64x2_all_true(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2AllTrue));
    }

    pub(crate) fn i64x2_bitmask(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Bitmask));
    }

    pub(crate) fn i64x2_extend_low_i32x4_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ExtendLowI32x4S));
    }

    pub(crate) fn i64x2_extend_high_i32x4_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ExtendHighI32x4S));
    }

    pub(crate) fn i64x2_extend_low_i32x4_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ExtendLowI32x4U));
    }

    pub(crate) fn i64x2_extend_high_i32x4_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ExtendHighI32x4U));
    }

    pub(crate) fn i64x2_shl(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2Shl));
    }

    pub(crate) fn i64x2_shr_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ShrS));
    }

    pub(crate) fn i64x2_shr_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ShrU));
    }

    pub(crate) fn i64x2_extmul_low_i32x4_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ExtmulLowI32x4S));
    }

    pub(crate) fn i64x2_extmul_high_i32x4_s(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ExtmulHighI32x4S));
    }

    pub(crate) fn i64x2_extmul_low_i32x4_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ExtmulLowI32x4U));
    }

    pub(crate) fn i64x2_extmul_high_i32x4_u(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2ExtmulHighI32x4U));
    }

    pub(crate) fn f32x4_ceil(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Ceil));
    }

    pub(crate) fn f32x4_floor(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Floor));
    }

    pub(crate) fn f32x4_trunc(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Trunc));
    }

    pub(crate) fn f32x4_nearest(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Nearest));
    }

    pub(crate) fn f32x4_abs(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Abs));
    }

    pub(crate) fn f32x4_neg(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Neg));
    }

    pub(crate) fn f32x4_sqrt(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Sqrt));
    }

    pub(crate) fn f32x4_div(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Div));
    }

    pub(crate) fn f32x4_pmin(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Pmin));
    }

    pub(crate) fn f32x4_pmax(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4Pmax));
    }

    pub(crate) fn f64x2_ceil(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Ceil));
    }

    pub(crate) fn f64x2_floor(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Floor));
    }

    pub(crate) fn f64x2_trunc(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Trunc));
    }

    pub(crate) fn f64x2_nearest(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Nearest));
    }

    pub(crate) fn f64x2_abs(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Abs));
    }

    pub(crate) fn f64x2_neg(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Neg));
    }

    pub(crate) fn f64x2_sqrt(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Sqrt));
    }

    pub(crate) fn f64x2_div(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Div));
    }

    pub(crate) fn f64x2_pmin(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Pmin));
    }

    pub(crate) fn f64x2_pmax(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2Pmax));
    }

    pub(crate) fn i32x4_trunc_sat_f32x4_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4TruncSatF32x4S));
    }

    pub(crate) fn i32x4_trunc_sat_f32x4_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4TruncSatF32x4U));
    }

    pub(crate) fn f32x4_convert_i32x4_s(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4ConvertI32x4S));
    }

    pub(crate) fn f32x4_convert_i32x4_u(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4ConvertI32x4U));
    }

    pub(crate) fn i32x4_trunc_sat_f64x2_s_zero(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4TruncSatF64x2SZero));
    }

    pub(crate) fn i32x4_trunc_sat_f64x2_u_zero(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4TruncSatF64x2UZero));
    }

    pub(crate) fn f64x2_convert_low_i32x4_s(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2ConvertLowI32x4S));
    }

    pub(crate) fn f64x2_convert_low_i32x4_u(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2ConvertLowI32x4U));
    }

    pub(crate) fn f32x4_demote_f64x2_zero(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4DemoteF64x2Zero));
    }

    pub(crate) fn f64x2_promote_low_f32x4(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2PromoteLowF32x4));
    }

    pub(crate) fn i8x16_relaxed_swizzle(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16RelaxedSwizzle));
    }

    pub(crate) fn i32x4_relaxed_trunc_f32x4_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4RelaxedTruncF32x4S));
    }

    pub(crate) fn i32x4_relaxed_trunc_f32x4_u(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4RelaxedTruncF32x4U));
    }

    pub(crate) fn i32x4_relaxed_trunc_f64x2_s_zero(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4RelaxedTruncF64x2SZero));
    }

    pub(crate) fn i32x4_relaxed_trunc_f64x2_u_zero(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4RelaxedTruncF64x2UZero));
    }

    pub(crate) fn f32x4_relaxed_madd(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4RelaxedMadd));
    }

    pub(crate) fn f32x4_relaxed_nmadd(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4RelaxedNmadd));
    }

    pub(crate) fn f64x2_relaxed_madd(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2RelaxedMadd));
    }

    pub(crate) fn f64x2_relaxed_nmadd(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2RelaxedNmadd));
    }

    pub(crate) fn i8x16_relaxed_laneselect(&mut self) {
        self.push(Op::Nullary(Nullary::I8x16RelaxedLaneselect));
    }

    pub(crate) fn i16x8_relaxed_laneselect(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8RelaxedLaneselect));
    }

    pub(crate) fn i32x4_relaxed_laneselect(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4RelaxedLaneselect));
    }

    pub(crate) fn i64x2_relaxed_laneselect(&mut self) {
        self.push(Op::Nullary(Nullary::I64x2RelaxedLaneselect));
    }

    pub(crate) fn f32x4_relaxed_min(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4RelaxedMin));
    }

    pub(crate) fn f32x4_relaxed_max(&mut self) {
        self.push(Op::Nullary(Nullary::F32x4RelaxedMax));
    }

    pub(crate) fn f64x2_relaxed_min(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2RelaxedMin));
    }

    pub(crate) fn f64x2_relaxed_max(&mut self) {
        self.push(Op::Nullary(Nullary::F64x2RelaxedMax));
    }

    pub(crate) fn i16x8_relaxed_q15mulr_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8RelaxedQ15mulrS));
    }

    pub(crate) fn i16x8_relaxed_dot_i8x16_i7x16_s(&mut self) {
        self.push(Op::Nullary(Nullary::I16x8RelaxedDotI8x16I7x16S));
    }

    pub(crate) fn i32x4_relaxed_dot_i8x16_i7x16_add_s(&mut self) {
        self.push(Op::Nullary(Nullary::I32x4RelaxedDotI8x16I7x16AddS));
    }
}
