//! Typed slot permutations preserve the owning array's complete set of tokens.

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use crate::Operand;
use crate::backend::shared::{abi_types::elem_size, panic_msgs};
use dream_types::TypeId;

impl Fx<'_, '_> {
    pub(super) fn array_slot_take(
        &mut self,
        elem_ty: TypeId,
        array: &Operand,
        index: &Operand,
    ) -> V {
        let size = elem_size(&self.l.cx, elem_ty) as i64;
        let array = self.operand(array);
        let index = self.operand(index);
        let index = V::s(self.conv(&index, &Ty::I32));
        let message = self.str_v(panic_msgs::INDEX_OUT_OF_BOUNDS);
        let at = self.panic_location();
        let slot = self.call_v("dream_array_at", &[array, index, V::i32(size), message, at]);
        let result = if self.is_value(elem_ty) {
            let temp = self.alloca_bytes(size as u64, 4);
            self.memcpy(&temp, &slot.v, &Value::i64(size));
            V::s(temp)
        } else {
            self.load_mem(
                crate::backend::shared::abi_types::mem_ty(&self.l.cx, elem_ty),
                &slot.v,
                4,
            )
        };
        self.memset0(&slot.v, &Value::i64(size));
        result
    }

    pub(super) fn array_slot_swap(
        &mut self,
        elem_ty: TypeId,
        array: &Operand,
        left: &Operand,
        right: &Operand,
    ) {
        let size = elem_size(&self.l.cx, elem_ty) as i64;
        let array = self.operand(array);
        let left = self.operand(left);
        let right = self.operand(right);
        let message = self.str_v(panic_msgs::INDEX_OUT_OF_BOUNDS);
        let at = self.panic_location();
        let left = V::s(self.conv(&left, &Ty::I32));
        let right = V::s(self.conv(&right, &Ty::I32));
        let a = self.call_v(
            "dream_array_at",
            &[
                array.clone(),
                left,
                V::i32(size),
                message.clone(),
                at.clone(),
            ],
        );
        let b = self.call_v("dream_array_at", &[array, right, V::i32(size), message, at]);
        let different = self.w.icmp("ne", &a.v, &b.v);
        self.if_then(&different, |fx| {
            // No user code or ownership observation can run while the permutation is open.
            let temp = fx.alloca_bytes(size as u64, 4);
            let size = Value::i64(size);
            fx.memcpy(&temp, &a.v, &size);
            fx.memcpy(&a.v, &b.v, &size);
            fx.memcpy(&b.v, &temp, &size);
        });
    }
}
