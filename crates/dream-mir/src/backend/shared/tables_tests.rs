use super::*;
use dream_hir::TypeLayout;
use dream_types::{DefKind, TypeCtx};

#[test]
fn nominal_tags_skip_the_builtin_range() {
    let mut types = TypeCtx::new();
    let mut mir = Mir::default();
    let mut ids = Vec::new();
    for i in 0..300 {
        let name = format!("Node{i}");
        let def = types.register(DefKind::Struct, &name, vec![]);
        let ty = types.interner.struct_ty(def, vec![]);
        mir.layouts.insert(
            ty,
            TypeLayout {
                name,
                ..Default::default()
            },
        );
        ids.push(ty);
    }
    let tags = struct_tags(&mir);
    let distinct: std::collections::BTreeSet<_> = tags.values().copied().collect();
    assert_eq!(distinct.len(), ids.len());
    for builtin in [
        crate::abi::TAG_FUTURE,
        crate::abi::TAG_FUNCBOX,
        crate::abi::TAG_CLOSURE_ENV,
        crate::abi::TAG_FLOAT16,
    ] {
        assert!(!distinct.contains(&builtin));
    }
    let boundary = (crate::abi::TAG_FUTURE - TAG_STRUCT_BASE) as usize;
    assert_eq!(tags[&ids[boundary - 1]], crate::abi::TAG_FUTURE - 1);
    assert_eq!(tags[&ids[boundary]], crate::abi::TAG_FLOAT16 + 1);
    assert_eq!(tags, struct_tags(&mir));
}
