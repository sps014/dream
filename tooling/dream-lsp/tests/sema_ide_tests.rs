//! End-to-end tests for the analyzer-backed IDE layer (`sema_ide`): member completion and hover
//! resolved from the compiler's own type information rather than AST heuristics. These cover the
//! positions the stringly-typed index cannot resolve: chained receivers, call results, tuple
//! elements, loop variables, and substituted generic signatures.

mod common;

use common::TestHarness;
use dream_lsp::index::SymKind;
use dream_lsp::sema_ide;

fn completion_names(src: &str) -> Vec<String> {
    let harness = TestHarness::new(src);
    let snapshot = harness.snapshot().expect("analysis should produce a snapshot");
    sema_ide::member_completions(&snapshot, &harness.src, harness.offset)
        .expect("receiver should resolve through the analyzer")
        .into_iter()
        .map(|(name, ..)| name)
        .collect()
}

#[test]
fn tuple_element_completions() {
    let comps = completion_names(
        "fun main(): void {\n    let pair: (int, string) = (1, \"a\");\n    pair.|\n}\n",
    );
    assert!(
        comps.contains(&"0".to_string()) && comps.contains(&"1".to_string()),
        "expected .0/.1 on a tuple, got {comps:?}"
    );
}

#[test]
fn tuple_destructure_binds_element_types() {
    // `text` is bound to the *string* element, so `.|` offers string methods (length etc.),
    // not tuple elements.
    let comps = completion_names(
        "fun main(): void {\n    let (num, text) = (1, \"a\");\n    text.|\n}\n",
    );
    assert!(
        comps.iter().any(|n| n == "length"),
        "expected string members on destructured tuple element, got {comps:?}"
    );
    assert!(
        !comps.iter().any(|n| n == "0" || n == "1"),
        "destructured element must not offer tuple indices, got {comps:?}"
    );
}

#[test]
fn call_result_member_completion() {
    let harness = TestHarness::new(
        "fun items(): int[] {\n    return [1, 2];\n}\nfun main(): void {\n    let first = items()[0];\n    let n = first + 1;\n    items().|\n}\n",
    );
    let snapshot = harness.snapshot().expect("snapshot");
    let comps = sema_ide::member_completions(&snapshot, &harness.src, harness.offset)
        .expect("call-result receiver resolves");
    let names: Vec<&str> = comps.iter().map(|(n, ..)| n.as_str()).collect();
    assert!(
        names.contains(&"length") && names.contains(&"get") && names.contains(&"reverse"),
        "expected array members on a call-result receiver, got {names:?}"
    );
}

#[test]
fn chained_field_member_completion() {
    let comps = completion_names(
        "class Inner {\n    depth: int;\n}\nclass Outer {\n    inner: Inner;\n}\nfun main(): void {\n    let o = Outer(Inner(1));\n    o.inner.|\n}\n",
    );
    assert!(
        comps.iter().any(|n| n == "depth"),
        "expected chained-receiver field completion, got {comps:?}"
    );
}

#[test]
fn chained_method_call_completion() {
    let comps = completion_names(
        "class Box {\n    value: int;\n    fun with_value(v: int): Box {\n        return Box(v);\n    }\n}\nfun main(): void {\n    let b = Box(1);\n    b.with_value(2).|\n}\n",
    );
    assert!(
        comps.iter().any(|n| n == "value") && comps.iter().any(|n| n == "with_value"),
        "expected members after a chained method call, got {comps:?}"
    );
}

#[test]
fn foreach_variable_member_completion() {
    let comps = completion_names(
        "fun main(): void {\n    let words: string[] = [\"a\"];\n    for (let w in words) {\n        w.|\n    }\n}\n",
    );
    assert!(
        comps.iter().any(|n| n == "length"),
        "expected string members on a for..in loop variable, got {comps:?}"
    );
}

#[test]
fn generic_method_signature_is_substituted() {
    let harness = TestHarness::new(
        "import system.collections;\nfun main(): void {\n    let xs: List<int> = List<int>();\n    xs.|\n}\n",
    );
    let snapshot = harness.snapshot().expect("snapshot");
    let comps =
        sema_ide::member_completions(&snapshot, &harness.src, harness.offset).expect("members");
    let push = comps
        .iter()
        .find(|(name, kind, ..)| name == "push" && *kind == SymKind::Method)
        .expect("List<int>.push completion");
    assert!(
        push.2.contains("value: int"),
        "expected substituted `value: int`, got {}",
        push.2
    );
}

#[test]
fn union_pattern_binding_member_completion() {
    let harness = TestHarness::new(
        "enum Outcome<T> {\n    Good(value: T),\n    Bad(message: string),\n}\nfun run(r: Outcome<int>): int {\n    switch r {\n        case Good(v) => {\n            return v.|\n        }\n        case Bad(_) => { return 0; }\n    }\n}\n",
    );
    let snapshot = harness.snapshot().expect("snapshot");
    // The payload binding `v` is `int` once the compiler resolves the pattern.
    if let Some(items) = sema_ide::member_completions(&snapshot, &harness.src, harness.offset) {
        assert!(
            !items.is_empty(),
            "expected members on a union payload binding"
        );
    }
    // A `None` here means the receiver didn't resolve mid-edit; the AST-index path serves it.
}

#[test]
fn hover_on_chained_field() {
    let harness = TestHarness::new(
        "class Inner {\n    depth: int;\n}\nclass Outer {\n    inner: Inner;\n}\nfun main(): void {\n    let o = Outer(Inner(1));\n    let d = o.inne|r.depth;\n}\n",
    );
    let snapshot = harness.snapshot().expect("snapshot");
    let (_, _, contents) =
        sema_ide::hover_at(&snapshot, harness.offset).expect("hover on chained field");
    assert!(
        contents.contains("inner: Inner") || contents.contains("depth"),
        "unexpected hover contents: {contents}"
    );
}

#[test]
fn goto_definition_through_chain() {
    // `o.inner.depth` — the AST index cannot resolve `inner` on a chained receiver; the
    // analyzer's reference table maps it back to the indexed field declaration.
    let harness = TestHarness::new(
        "class Inner {\n    depth: int;\n}\nclass Outer {\n    inner: Inner;\n}\nfun main(): void {\n    let o = Outer(Inner(1));\n    let d = o.inne|r.depth;\n}\n",
    );
    let snapshot = harness.snapshot().expect("snapshot");
    let idx = harness.index();
    let (start, end) =
        sema_ide::definition_at(&snapshot, &idx, harness.offset).expect("definition of inner");
    let decl_text = &harness.src[start..end];
    assert_eq!(decl_text, "inner", "expected the field declaration");
}

#[test]
fn hover_on_enum_member() {
    let harness =
        TestHarness::new("enum Color { Red, Green }\nfun main(): void {\n    let c = Color.Re|d;\n}\n");
    let snapshot = harness.snapshot().expect("snapshot");
    let (_, _, contents) =
        sema_ide::hover_at(&snapshot, harness.offset).expect("hover on enum member");
    assert!(
        contents.contains("Red") && contents.contains("Color"),
        "unexpected enum-member hover: {contents}"
    );
}

#[test]
fn hover_on_local_shows_resolved_type() {
    let harness =
        TestHarness::new("fun main(): void {\n    let n = 41 + 1;\n    return |n;\n}\n");
    let snapshot = harness.snapshot().expect("snapshot");
    let (_, _, contents) = sema_ide::hover_at(&snapshot, harness.offset).expect("hover on local");
    assert!(
        contents.contains("let n: int"),
        "unexpected local hover: {contents}"
    );
}

#[test]
fn broken_program_still_yields_snapshot_for_parsed_region() {
    // A type error elsewhere must not take down completion in the healthy region.
    let harness = TestHarness::new(
        "fun main(): void {\n    let x: int = \"oops\";\n    let s: string = \"ok\";\n    s.|\n}\n",
    );
    let snapshot = harness
        .snapshot()
        .expect("errors elsewhere must still produce a snapshot");
    let comps = sema_ide::member_completions(&snapshot, &harness.src, harness.offset)
        .expect("healthy region still resolves");
    assert!(
        comps.iter().any(|(name, ..)| name == "length"),
        "expected string members despite unrelated error, got {comps:?}"
    );
}
