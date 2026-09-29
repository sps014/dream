use std::path::PathBuf;

use bumpalo::Bump;
use dream_diagnostics::DiagnosticBag;

use crate::driver::native_sets::NativeGraph;
use crate::driver::source_loader::{parse_file_recursive, ProgramAccumulator};

/// A package `kv` with one C++ source and `src/main.dream` holding `source`.
fn package(tag: &str, source: &str, with_native: bool) -> PathBuf {
    let root = std::env::temp_dir().join(format!("dream-cpp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("dream.toml"), "[package]\nname = \"kv\"\n").unwrap();
    if with_native {
        std::fs::create_dir_all(root.join("native")).unwrap();
        std::fs::write(root.join("native/kv.cpp"), "").unwrap();
    }
    let main = root.join("src/main.dream");
    std::fs::write(&main, source).unwrap();
    main
}

/// Runs the expansion over `source`; returns the `kv` shim and the rendered diagnostics.
fn expand(tag: &str, source: &str, with_native: bool) -> (Option<String>, String) {
    let main = package(tag, source, with_native);
    let arena = Bump::new();
    let mut acc = ProgramAccumulator::default();
    let mut diagnostics = DiagnosticBag::new(None);
    let path = main.to_str().unwrap().to_string();
    parse_file_recursive(&path, &mut acc, &arena, &mut diagnostics).unwrap();
    let graph = NativeGraph::load(&path, &acc).unwrap();
    let bridge = super::expand(&arena, &mut acc, &graph, &mut diagnostics).unwrap();
    let messages = diagnostics
        .errors()
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::remove_dir_all(main.parent().unwrap().parent().unwrap());
    (bridge.shim("kv").map(str::to_string), messages)
}

#[test]
fn shim_covers_each_member_shape() {
    let (shim, messages) = expand(
        "shapes",
        "import system;\n\
         @cpp(\"kv.hpp\", \"kv::Store\")\n\
         public class Store {\n\
             extern constructor(path: string);\n\
             extern fun put(key: string, value: string): void;\n\
             extern fun get(key: string): Option<string>;\n\
             extern fun compact(): Result<bool, string>;\n\
             extern fun find(key: string): Option<Store>;\n\
             extern fun on_change(f: fun(string): void): void;\n\
             @cpp_name(\"get_as<int>\") extern fun get_int(key: string): int;\n\
             @owned static extern fun open(): Store;\n\
         }\n\
         @cpp(\"kv.hpp\", \"kv::greet\")\n\
         extern fun greet(who: string): string;\n",
        true,
    );
    assert_eq!(messages, "");
    let shim = shim.unwrap();
    for needle in [
        "#include \"dream_bridge.hpp\"",
        "#include \"kv.hpp\"",
        "extern \"C\" void* dream__kv__Store____new(",
        "new kv::Store(",
        "dream__kv__Store__put(",
        "dream::out_opt_str",
        "dream::attempt<",
        "dream::out_obj<kv::Store",
        "dream::callback<",
        "->get_as<int>(",
        "kv::Store::open()",
        "kv::greet(",
        "dream__kv__Store____delete(",
        "#line 5 ",
    ] {
        assert!(shim.contains(needle), "missing {:?} in:\n{}", needle, shim);
    }
}

#[test]
fn non_bridgeable_and_non_extern_members_are_rejected() {
    let (_, messages) = expand(
        "reject",
        "@cpp(\"kv.hpp\")\n\
         class Store {\n\
             count: int;\n\
             extern fun keys(m: Map<string, int>): void;\n\
             fun helper(): int { return 1; }\n\
             extern fun opt(n: int = 3): void;\n\
         }\n",
        true,
    );
    for needle in [
        "`@cpp` class 'Store' holds only `extern` members; keep Dream state",
        "`@cpp` member 'keys' parameter 'm'",
        "put helpers such as 'helper' in an `extend Store { ... }` block",
        "`@cpp` parameter 'n' cannot have a default",
    ] {
        assert!(messages.contains(needle), "missing {:?} in:\n{}", needle, messages);
    }
}

#[test]
fn cpp_without_native_sources_is_reported() {
    let (shim, messages) = expand(
        "nosrc",
        "@cpp(\"kv.hpp\", \"kv::greet\")\nextern fun greet(who: string): string;\n",
        false,
    );
    assert!(shim.is_none());
    assert!(
        messages.contains("`@cpp` on 'greet' needs C/C++ sources in a `native/` directory"),
        "{}",
        messages
    );
}
