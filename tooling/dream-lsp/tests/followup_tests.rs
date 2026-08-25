//! Tests for the follow-up IDE features: structured diagnostic codes, embedded-stdlib
//! navigation, workspace disk scan, and type-safe (entity-identity) reference matching.

mod common;

use common::TestHarness;
use dream_lsp::analysis::analyze_document;
use dream_lsp::sema_ide;
use std::fs;

// ---------------------------------------------------------------- structured codes

#[test]
fn unresolved_identifier_carries_code() {
    let outcome = analyze_document(None, "fun main(): void {\n    undefined_thing;\n}\n");
    let diag = outcome
        .diagnostics
        .iter()
        .find(|d| d.message.contains("undefined_thing"))
        .expect("unresolved-name diagnostic");
    assert_eq!(diag.code, Some("unresolved-name"));
}

#[test]
fn missing_member_carries_code_not_unresolved_name() {
    let src = "class Counter {\n    count: int;\n}\nfun main(): void {\n    let c = Counter(1);\n    let n = c.nope;\n}\n";
    for d in analyze_document(None, src).diagnostics {
        if d.message.contains("nope") {
            assert_eq!(d.code, Some("missing-member"), "{}", d.message);
            return;
        }
    }
    panic!("expected a missing-member diagnostic");
}

#[test]
fn unrelated_errors_have_no_code() {
    // Type mismatches are not resolution failures: they must stay uncoded so code actions
    // never offer auto-import on them.
    let outcome =
        analyze_document(None, "fun main(): void {\n    let x: int = \"oops\";\n}\n");
    let diag = outcome
        .diagnostics
        .iter()
        .find(|d| d.message.contains("oops") || d.message.contains("int"))
        .expect("type-mismatch diagnostic");
    assert_eq!(diag.code, None);
}

// ---------------------------------------------------------------- stdlib navigation

#[test]
fn materializes_stdlib_source_for_navigation() {
    let dir = std::env::temp_dir().join(format!(
        "dream-lsp-test-std-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    let path = dream_lsp::backend::materialize_stdlib(&dir, "<std>/system/collections/list.dream")
        .expect("embedded list.dream should resolve");
    let text = fs::read_to_string(&path).expect("materialized file should exist");
    assert!(text.contains("class List"), "unexpected content: {}", &text[..200.min(text.len())]);

    // Unknown virtual paths fail cleanly.
    assert!(dream_lsp::backend::materialize_stdlib(&dir, "<std>/nope/nope.dream").is_none());
    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- workspace scan

#[test]
fn scans_workspace_files_for_symbols() {
    let root = std::env::temp_dir().join(format!("dream-lsp-ws-{}", std::process::id()));
    let sub = root.join("lib");
    fs::create_dir_all(&sub).expect("mkdir");
    fs::write(
        root.join("main.dream"),
        "fun main(): void {}\nclass Helper {}\n",
    )
    .unwrap();
    fs::write(sub.join("util.dream"), "public fun util_add(a: int, b: int): int { return a + b; }\n").unwrap();
    // Skipped directories must not contribute.
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("target/junk.dream"), "class ShouldNotAppear {}\n").unwrap();

    let symbols = dream_lsp::workspace::scan(&root);
    let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"main"), "{names:?}");
    assert!(names.contains(&"Helper"), "{names:?}");
    assert!(names.contains(&"util_add"), "{names:?}");
    assert!(!names.contains(&"ShouldNotAppear"), "{names:?}");

    let main_sym = symbols.iter().find(|s| s.name == "main").unwrap();
    let text = fs::read_to_string(root.join("main.dream")).unwrap();
    assert_eq!(&text[main_sym.start..main_sym.end], "main");

    let _ = fs::remove_dir_all(&root);
}

// ------------------------------------------------- type-safe (identity) references

#[test]
fn same_named_fields_do_not_collide_in_references() {
    let src = "class Point {\n    x: int;\n}\nclass Size {\n    x: int;\n}\nfun main(): void {\n    let p = Point(1);\n    let s = Size(2);\n    let a = p.x;\n    let b = s.x;\n    let t = a + b;\n}\n";
    let snapshot = common::snapshot_of(src).expect("snapshot");

    // Query the `s.x` access directly: only that use may match. The legacy bare-name match
    // would also return `a` (p.x).
    let sx_pos = src.find("s.x").unwrap() + "s.".len();
    let r = snapshot.ref_covering(sx_pos).expect("field use under cursor");
    let spans = sema_ide::references_in(&snapshot, &r.target);
    assert_eq!(spans.len(), 1, "only the matching field's uses, got {spans:?}");
    let (start, end) = spans[0];
    assert_eq!(&src[start..end], "x");
}

#[test]
fn method_references_match_by_declaring_type() {
    let harness = TestHarness::new(
        "class Dog {\n    fun speak(): int { return 1; }\n}\nclass Cat {\n    fun speak(): int { return 2; }\n}\nfun main(): void {\n    let d = Dog();\n    let c = Cat();\n    let a = d.speak();\n    let b = c.speak();\n    let t = |a + b;\n}\n",
    );
    let snapshot = harness.snapshot().expect("snapshot");
    // Resolve through the `d.speak()` call site.
    let speak_pos = harness.src.find("d.speak").unwrap() + "d.".len();
    let r = snapshot
        .ref_covering(speak_pos)
        .expect("method call ref recorded");
    let spans = sema_ide::references_in(&snapshot, &r.target);
    assert_eq!(spans.len(), 1, "Dog.speak only, got {spans:?}");
    let (start, end) = spans[0];
    assert_eq!(&harness.src[start..end], "speak");
}

#[test]
fn foreign_file_refs_are_excluded_from_primary_doc() {
    // The merged program includes prelude bodies; their spans must never surface as
    // locations in the user's document.
    let harness = TestHarness::new(
        "import system.collections;\nfun main(): void {\n    let xs = List<int>();\n    xs.push(1);\n    let n = xs.le|ngth;\n}\n",
    );
    let snapshot = harness.snapshot().expect("snapshot");
    let push_pos = harness.src.find("xs.push").unwrap() + "xs.".len();
    let r = snapshot.ref_covering(push_pos).expect("push call ref");
    let spans = sema_ide::references_in(&snapshot, &r.target);
    for (start, end) in spans {
        let slice = &harness.src[start..end];
        assert_eq!(slice, "push", "non-primary span leaked into results");
    }
}
