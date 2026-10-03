use super::*;
use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::{lexer::Lexer, parser::Parser};

fn parse<'a>(arena: &'a Bump, source: &str) -> ProgramNode<'a> {
    let mut diagnostics = DiagnosticBag::new(None);
    let tree = Parser::new(Lexer::new(source.into()), arena, &mut diagnostics)
        .parse()
        .unwrap();
    assert!(!diagnostics.has_errors());
    tree.get_root().clone()
}

#[test]
fn module_scoped_nominals_have_distinct_identities() {
    let arena = Bump::new();
    let graph = ModuleGraph::new(
        ["a", "b"]
            .iter()
            .map(|name| {
                let source = format!("module {name}; public class User {{}}");
                (
                    format!("{name}.dream"),
                    source.clone(),
                    parse(&arena, &source),
                )
            })
            .collect(),
        &IndexMap::new(),
    );
    let mut types = TypeCtx::new();
    graph.configure_types(&mut types);
    let ids: Vec<_> = graph
        .files
        .iter()
        .map(|file| {
            types.set_scope(file.module);
            types.register(dream_types::DefKind::Struct, "User", vec![])
        })
        .collect();
    assert_ne!(ids[0], ids[1]);
    assert_eq!(ids[0].index, ids[1].index);
    for (file, expected) in graph.files.iter().zip(ids) {
        types.set_scope(file.module);
        assert_eq!(
            types.resolve(dream_types::DefKind::Struct, "User"),
            Some(expected)
        );
    }
}

#[test]
fn interface_hash_ignores_bodies_and_source_positions() {
    let arena = Bump::new();
    let sources = [
        "module a; public fun value(): int { return 1; }",
        "\nmodule a;\npublic fun value(): int { return 2; }",
        "module a; public fun value(): long { return 2; }",
    ];
    let graphs: Vec<_> = sources
        .iter()
        .map(|source| {
            ModuleGraph::new(
                vec![("a.dream".into(), source.to_string(), parse(&arena, source))],
                &IndexMap::new(),
            )
        })
        .collect();
    assert_ne!(
        graphs[0].modules[1].content_hash,
        graphs[1].modules[1].content_hash
    );
    assert_eq!(
        graphs[0].modules[1].interface_hash,
        graphs[1].modules[1].interface_hash
    );
    assert_ne!(
        graphs[0].modules[1].interface_hash,
        graphs[2].modules[1].interface_hash
    );
}
fn graph<'a>(
    arena: &'a Bump,
    sources: &[(&str, &str)],
    edges: &[(&str, &[&str])],
) -> ModuleGraph<'a> {
    ModuleGraph::new(
        sources
            .iter()
            .map(|(path, source)| (path.to_string(), source.to_string(), parse(arena, source)))
            .collect(),
        &edges
            .iter()
            .map(|(path, dependencies)| {
                (
                    path.to_string(),
                    dependencies.iter().map(|s| s.to_string()).collect(),
                )
            })
            .collect(),
    )
}

fn interface(source: &str) -> [u8; 32] {
    let arena = Bump::new();
    graph(&arena, &[("a.dream", source)], &[]).modules[1].interface_hash
}

#[test]
fn exported_globals_extensions_and_layout_contracts_invalidate_interfaces() {
    for (before, after) in [
        ("public let x: int = 1;", "public let x: long = 1;"),
        ("public let x: int = 1;", "public const x: int = 1;"),
        ("public const x: int = 1;", "public const x: int = 2;"),
        ("public let x = 1;", "public let x = true;"),
        (
            "extend int { public static fun x(): int { return 1; } }",
            "extend int { public static fun x(): long { return 1; } }",
        ),
        ("public class C {}", "public sealed class C {}"),
        (
            "public fun f(x: int = 1): int { return x; }",
            "public fun f(x: int = 2): int { return x; }",
        ),
        (
            "public fun f<T>(x: T): T { return x; }",
            "public fun f<T>(x: T): T { let y = x; return y; }",
        ),
        (
            "public class C { public x: Box<int>; }",
            "public class C { public x: Box_int; }",
        ),
    ] {
        assert_ne!(
            interface(&format!("module a; {before}")),
            interface(&format!("module a; {after}")),
            "{before}"
        );
    }
}

#[test]
fn explicit_mutable_global_initializers_are_not_interface_semantics() {
    assert_eq!(
        interface("module a; public let x: int = 1;"),
        interface("module a; public let x: int = 2;")
    );
}

#[test]
fn conservative_implementation_hash_ignores_trivia() {
    assert_eq!(
        interface("module a; public const x = 1;"),
        interface("\nmodule a; // comment\n public const x = 1;")
    );
}

#[test]
fn dependency_keys_track_interfaces_content_and_transitive_changes() {
    let arena = Bump::new();
    let a = "module a; public fun a(): int { return 1; }";
    let b = "module b; public fun b(): int { return 1; }";
    let c = "module c; public fun c(): int { return 1; }";
    let edges: &[(&str, &[&str])] = &[("a.dream", &["b.dream"]), ("b.dream", &["c.dream"])];
    let base = graph(
        &arena,
        &[("a.dream", a), ("b.dream", b), ("c.dream", c)],
        edges,
    );
    let body = graph(
        &arena,
        &[
            ("a.dream", a),
            ("b.dream", b),
            ("c.dream", "module c; public fun c(): int { return 2; }"),
        ],
        edges,
    );
    let signature = graph(
        &arena,
        &[
            ("a.dream", a),
            ("b.dream", b),
            ("c.dream", "module c; public fun c(): long { return 2; }"),
        ],
        edges,
    );
    assert_eq!(base.modules[1].cache_key(), body.modules[1].cache_key());
    assert_ne!(base.modules[3].cache_key(), body.modules[3].cache_key());
    assert_ne!(
        base.modules[1].cache_key(),
        signature.modules[1].cache_key()
    );
    assert_eq!(base.modules[1].dependency_interfaces.len(), 2);
}

#[test]
fn dependency_keys_are_order_independent_and_cycles_terminate() {
    let arena = Bump::new();
    let sources = [
        ("a.dream", "module a; public class A {}"),
        ("b.dream", "module b; public class B {}"),
        ("c.dream", "module c; public class C {}"),
    ];
    let left = graph(
        &arena,
        &sources,
        &[
            ("a.dream", &["b.dream", "c.dream"]),
            ("b.dream", &["a.dream"]),
        ],
    );
    let mut right = graph(
        &arena,
        &[sources[2], sources[1], sources[0]],
        &[
            ("a.dream", &["c.dream", "b.dream"]),
            ("b.dream", &["a.dream"]),
        ],
    );
    let a = |g: &ModuleGraph<'_>| {
        g.modules
            .iter()
            .find(|m| m.path == "a")
            .unwrap()
            .cache_key()
    };
    assert_eq!(a(&left), a(&right));
    right.prepare_dependency_interfaces();
    assert_eq!(a(&left), a(&right));
    let b = right.modules.iter().find(|m| m.path == "b").unwrap();
    assert!(b.dependency_interfaces.iter().all(|(path, _)| path != "b"));
}

#[test]
fn file_identity_and_prepared_ast_changes_invalidate_content_keys() {
    let arena = Bump::new();
    let source = "module a; public class A {}";
    let mut base = graph(&arena, &[("a.dream", source)], &[]);
    let renamed = graph(&arena, &[("other.dream", source)], &[]);
    assert_ne!(base.modules[1].cache_key(), renamed.modules[1].cache_key());
    base.files[0].program.structs[0].is_sealed = true;
    let prepared = ModuleGraph::new(
        vec![(
            "a.dream".into(),
            source.into(),
            base.files[0].program.clone(),
        )],
        &IndexMap::new(),
    );
    assert_ne!(base.modules[1].cache_key(), prepared.modules[1].cache_key());
}

#[test]
fn inferred_receiver_and_default_body_changes_invalidate_interfaces() {
    for (before, after) in [
        (
            "public class C { public x: int; public fun f(): int { return this.x; } }",
            "public class C { public x: int; public fun f(): int { this.x = 2; return this.x; } }",
        ),
        (
            "public interface I { borrow fun f(): int { return 1; } }",
            "public interface I { borrow fun f(): int { return 2; } }",
        ),
    ] {
        assert_ne!(
            interface(&format!("module a; {before}")),
            interface(&format!("module a; {after}"))
        );
    }
}

#[test]
fn implementation_dependent_interfaces_include_other_files_in_the_module() {
    let arena = Bump::new();
    let exported = "module a; public fun f<T>(x: T): T { helper(); return x; }";
    let before = graph(
        &arena,
        &[
            ("a.dream", exported),
            ("helper.dream", "module a; fun helper(): int { return 1; }"),
        ],
        &[],
    );
    let after = graph(
        &arena,
        &[
            ("a.dream", exported),
            ("helper.dream", "module a; fun helper(): int { return 2; }"),
        ],
        &[],
    );
    assert_ne!(
        before.modules[1].interface_hash,
        after.modules[1].interface_hash
    );
}

#[test]
fn aliased_imports_prepare_dependency_interfaces_without_file_edges() {
    let arena = Bump::new();
    let entry = "module a; import b.value as value; public fun f(): int { return value(); }";
    let before = graph(
        &arena,
        &[
            ("a.dream", entry),
            ("b.dream", "module b; public fun value(): int { return 1; }"),
        ],
        &[],
    );
    let after = graph(
        &arena,
        &[
            ("a.dream", entry),
            (
                "b.dream",
                "module b; public fun value(): long { return 1; }",
            ),
        ],
        &[],
    );
    assert_eq!(before.modules[1].dependency_interfaces.len(), 1);
    assert_ne!(before.modules[1].cache_key(), after.modules[1].cache_key());
}
