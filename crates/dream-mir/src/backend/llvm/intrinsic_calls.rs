//! Intrinsic calls emitted inline instead of as runtime calls: `@llvm("llvm.*")` externs become a
//! call to the named LLVM intrinsic, and the `double` bit reinterpretations become a `bitcast`.

use super::fx::{Fx, V};
use super::ir::{FnTy, Ty, Value};
use super::types::ll_ty;
use crate::{Callee, Operand};
use dream_abi::intrinsics::FloatOp;

impl<'l, 'a> Fx<'l, 'a> {
    pub fn float_op(&mut self, op: FloatOp, args: &[Operand]) -> V {
        let v = self.operand(&args[0]).v;
        match op {
            FloatOp::ToBits => V::s(self.w.cast("bitcast", &v, Ty::I64)),
            FloatOp::FromBits => V::s(self.w.cast("bitcast", &v, Ty::F64)),
        }
    }

    /// Sema restricts `@llvm` externs to numeric parameters and results, so the extern's own
    /// signature is the intrinsic's overload.
    pub fn llvm_intrinsic_call(&mut self, name: &str, callee: &Callee, args: &[Operand]) -> V {
        let (h, word) = (self.h(), self.word());
        let params = args
            .iter()
            .map(|a| ll_ty(self.interner, self.operand_ty(a), &h, &word))
            .collect();
        let ret = ll_ty(self.interner, callee.ret, &h, &word);
        let sig = self.l.intrinsic(name, FnTy::new(ret, params));
        let vals = args.iter().map(|a| self.operand(a).v).collect();
        let result = self
            .call_ptr(&Value::global(name), &sig, vals)
            .unwrap_or_else(|| crate::internal_error!("{name} returned void"));
        V::s(result)
    }
}
