//! Iterator identity is nominal: qualified annotations and local names must not change
//! whether an async call returns a sequence or a Future.

use dream_syntax::nodes::FunctionNode;
use dream_types::{DefKind, TyKind, TypeCtx, TypeId};

pub const ITERABLE: &str = "system.core::Iterable";
pub const ASYNC_ITERABLE: &str = "system.core::AsyncIterable";
pub const CHANNEL: &str = "system.core::YieldChannel";
pub const SEQUENCE: &str = "system.core::YieldIterable";
pub const ASYNC_SEQUENCE: &str = "system.core::YieldAsyncIterable";
pub const LEASE: &str = "system.core::IteratorLease";
pub const ASYNC_LEASE: &str = "system.core::AsyncIteratorLease";

pub fn element(ctx: &TypeCtx, ty: TypeId, asynchronous: bool) -> Option<TypeId> {
    let TyKind::Interface(def, args) = ctx.interner.kind(ty) else {
        return None;
    };
    let expected = ctx.resolve(
        DefKind::Interface,
        if asynchronous {
            ASYNC_ITERABLE
        } else {
            ITERABLE
        },
    );
    (args.len() == 1 && Some(*def) == expected).then(|| args[0])
}

pub fn returns_future(ctx: &mut TypeCtx, function: &FunctionNode<'_>) -> bool {
    if !function.is_async {
        return false;
    }
    let ret = function
        .return_type
        .as_ref()
        .map(|ty| ctx.lower(ty))
        .unwrap_or_else(|| ctx.interner.void());
    element(ctx, ret, true).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dream_types::ModuleId;

    #[test]
    fn iterator_classification_uses_the_bootstrap_definition() {
        let mut ctx = TypeCtx::new();
        let standard = ModuleId(1);
        ctx.define_module(standard, "system.core".into(), Vec::new());
        ctx.set_scope(standard);
        let builtin = ctx.register(DefKind::Interface, "AsyncIterable", vec!["T".into()]);
        let int = ctx.interner.int();
        let builtin_type = ctx.interner.interface_ty(builtin, vec![int]);
        ctx.set_scope(ModuleId::ROOT);
        let user = ctx.register(DefKind::Interface, "AsyncIterable", vec!["T".into()]);
        let user_type = ctx.interner.interface_ty(user, vec![int]);
        assert_eq!(element(&ctx, builtin_type, true), Some(int));
        assert_eq!(element(&ctx, user_type, true), None);
        assert_eq!(element(&ctx, builtin_type, false), None);
    }
}
