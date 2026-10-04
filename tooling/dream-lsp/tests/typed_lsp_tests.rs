use dream_lsp::{
    analysis::analyze_document,
    index::{Index, SymKind},
    sema_ide,
};
use dream_sema::analyzer::ide::{IdeRef, IdeSnapshot, IdeSource, IdeTarget, TypeSummary};
use dream_types::{DefId, ModuleId, TypeId};

fn variable_type(source: &str, name: &str) -> String {
    let index = Index::build(None, source);
    index
        .decls
        .iter()
        .find(|d| d.kind == SymKind::Variable && d.name == name)
        .and_then(|d| d.ty.clone())
        .expect("inferred variable type")
}

#[test]
fn async_function_returning_callable_preserves_the_whole_return_type() {
    let source = "async fun get_callback(): fun(int): string { } async fun main(): void { let pending = get_callback(); let callback = pending.await; let result = callback(1); }";
    assert_eq!(variable_type(source, "pending"), "Future<fun(int): string>");
    assert_eq!(variable_type(source, "callback"), "fun(int): string");
    assert_eq!(variable_type(source, "result"), "string");
}

#[test]
fn generic_async_free_function_substitutes_all_tuple_elements() {
    let source = "async fun pair<A, B>(): (A, B) { } async fun main(): void { let pending = pair<int, string>(); let value = pending.await; }";
    assert_eq!(variable_type(source, "pending"), "Future<(int, string)>");
    assert_eq!(variable_type(source, "value"), "(int, string)");
}

#[test]
fn generic_async_method_keeps_receiver_and_method_bindings_separate() {
    let source = "class Box<T> { public value: T; public async fun pair<U>(other: U): (T, U) { } } async fun main(): void { let box: Box<int> = Box<int>(1); let pending = box.pair<string>(\"x\"); let value = pending.await; }";
    assert_eq!(variable_type(source, "pending"), "Future<(int, string)>");
    assert_eq!(variable_type(source, "value"), "(int, string)");
}

#[test]
fn resolved_sources_match_across_snapshot_local_ids_but_not_member_spans() {
    let source = IdeSource {
        file: Some("/project/model.dream".into()),
        start: 10,
        end: 15,
    };
    let target = |index, source| IdeTarget::Resolved {
        def: DefId {
            module: ModuleId::ROOT,
            index,
        },
        source,
        target: Box::new(IdeTarget::Field {
            owner: TypeId(40),
            name: "value".into(),
        }),
    };
    let first = target(1, source.clone());
    assert!(sema_ide::target_matches(&first, &target(7, source.clone())));
    assert!(!sema_ide::target_matches(
        &first,
        &target(
            1,
            IdeSource {
                start: 30,
                end: 35,
                ..source
            }
        )
    ));
}

#[test]
fn chained_field_rename_uses_the_resolved_source_span() {
    let source = "class Item { public value: int; } fun main(): void { get_item().value; }";
    let index = Index::build(None, source);
    let declaration = source.find("value").unwrap();
    let usage = source.rfind("value").unwrap();
    let target = IdeTarget::Resolved {
        def: DefId {
            module: ModuleId::ROOT,
            index: 1,
        },
        source: IdeSource {
            file: Some("main.dream".into()),
            start: declaration,
            end: declaration + 5,
        },
        target: Box::new(IdeTarget::Field {
            owner: TypeId(41),
            name: "value".into(),
        }),
    };
    let snapshot = IdeSnapshot {
        primary_file: Some("main.dream".into()),
        refs: vec![IdeRef {
            start: usage,
            end: usage + 5,
            file: Some("main.dream".into()),
            target: target.clone(),
            result: TypeSummary::Unknown,
        }],
        ..IdeSnapshot::default()
    };
    assert_eq!(
        sema_ide::rename_decl_at(&snapshot, &index, usage)
            .unwrap()
            .start,
        declaration
    );
    assert_eq!(
        sema_ide::references_in(&snapshot, &target),
        vec![(usage, usage + 5)]
    );
}

#[test]
fn generic_field_instances_resolve_to_one_declaration_without_matching_other_fields() {
    let source = "class Box<T> { public value: T; } class Other { public value: int; } fun main(): void { let a = Box<int>(1); let b = Box<string>(\"x\"); let c = Other(2); a.value; b.value; c.value; }";
    let outcome = analyze_document(None, source);
    let snapshot = outcome.sema.expect("semantic snapshot");
    let first = source.find("a.value").unwrap() + 2;
    let second = source.find("b.value").unwrap() + 2;
    let other = source.find("c.value").unwrap() + 2;
    let a = &snapshot
        .ref_covering(first)
        .expect("first field reference")
        .target;
    let b = &snapshot
        .ref_covering(second)
        .expect("second field reference")
        .target;
    let c = &snapshot
        .ref_covering(other)
        .expect("other field reference")
        .target;
    assert!(matches!(a, IdeTarget::Resolved { .. }));
    assert!(sema_ide::target_matches(a, b));
    assert!(!sema_ide::target_matches(a, c));
    let declaration = source.find("value").unwrap();
    assert!(sema_ide::target_matches(
        a,
        &snapshot
            .ref_covering(declaration)
            .expect("field declaration")
            .target
    ));
    assert_eq!(
        sema_ide::references_in(&snapshot, a),
        vec![(first, first + 5), (second, second + 5)]
    );
}

#[test]
fn generic_method_instances_share_the_template_source() {
    let source = "class Container { public fun identity<T>(value: T): T { return value; } } class Other { public fun identity<T>(value: T): T { return value; } } fun main(): void { let c = Container(); let o = Other(); c.identity<int>(1); c.identity<string>(\"x\"); o.identity<int>(2); }";
    let outcome = analyze_document(None, source);
    let snapshot = outcome.sema.expect("semantic snapshot");
    let first = source.find("c.identity<int>").unwrap() + 2;
    let second = source.find("c.identity<string>").unwrap() + 2;
    let other = source.find("o.identity<int>").unwrap() + 2;
    let a = &snapshot
        .ref_covering(first)
        .expect("first generic method")
        .target;
    let b = &snapshot
        .ref_covering(second)
        .expect("second generic method")
        .target;
    let c = &snapshot
        .ref_covering(other)
        .expect("other generic method")
        .target;
    assert!(matches!(a, IdeTarget::Resolved { .. }));
    assert!(sema_ide::target_matches(a, b));
    assert!(!sema_ide::target_matches(a, c));
    let declaration = source.find("identity").unwrap();
    assert_eq!(
        sema_ide::definition_at(&snapshot, &Index::build(None, source), first),
        Some((declaration, declaration + 8))
    );
    assert!(sema_ide::target_matches(
        a,
        &snapshot
            .ref_covering(declaration)
            .expect("method declaration")
            .target
    ));
}
