use super::modref::ModRefTable;
use crate::Mir;
use dream_hir::{InterfaceImpl, InterfaceInfo, TypeLayout};
use dream_types::{DefKind, TypeCtx};

#[test]
fn interface_cleanup_uses_the_complete_concrete_implementation_set() {
    let mut ctx = TypeCtx::new();
    let interface = ctx.register(DefKind::Interface, "I", vec![]);
    let a = ctx.register(DefKind::Struct, "A", vec![]);
    let b = ctx.register(DefKind::Struct, "B", vec![]);
    let iface = ctx.interner.interface_ty(interface, vec![]);
    let a = ctx.interner.struct_ty(a, vec![]);
    let b = ctx.interner.struct_ty(b, vec![]);
    let mut mir = Mir::default();
    mir.interfaces.interfaces.push(InterfaceInfo {
        ty: iface,
        name: "I".into(),
        method_count: 0,
        sigs: vec![],
    });
    mir.interfaces.impls.push(InterfaceImpl {
        class_ty: a,
        entries: vec![(0, vec![])],
    });
    mir.layouts.insert(a, TypeLayout::default());
    mir.layouts.insert(
        b,
        TypeLayout {
            destructor: Some(dream_types::DefId::root(100)),
            ..Default::default()
        },
    );
    let table = ModRefTable::compute(&mir, &ctx.interner);
    assert!(!table.may_run_del(iface, &ctx.interner, &mir.layouts));
    assert!(table.may_run_del(ctx.interner.object(), &ctx.interner, &mir.layouts));
    mir.interfaces.impls.push(InterfaceImpl {
        class_ty: b,
        entries: vec![(0, vec![])],
    });
    let table = ModRefTable::compute(&mir, &ctx.interner);
    assert!(table.may_run_del(iface, &ctx.interner, &mir.layouts));
    mir.interfaces.impls.clear();
    let table = ModRefTable::compute(&mir, &ctx.interner);
    assert!(table.may_run_del(iface, &ctx.interner, &mir.layouts));
}
