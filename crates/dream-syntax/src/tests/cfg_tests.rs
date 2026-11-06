use super::*;
use crate::conditional::{CfgSymbols, TargetSymbol};
use crate::lexer::Lexer;

struct Parsed {
    functions: Vec<String>,
    globals: Vec<String>,
    imports: usize,
    fields: Vec<String>,
    methods: Vec<String>,
    variants: Vec<String>,
    interface_methods: Vec<String>,
    extend_methods: Vec<String>,
    main_statements: usize,
    inactive: Vec<(usize, usize)>,
    doc_trivia: bool,
    errors: Vec<String>,
}

fn parse_with(code: &str, cfg: CfgSymbols) -> Parsed {
    let arena = bumpalo::Bump::new();
    let mut diagnostics = DiagnosticBag::new(None);
    let mut parser =
        Parser::new(Lexer::new(code.to_string()), &arena, &mut diagnostics).with_cfg(cfg);
    let tree = parser.parse().ok();
    let root = tree.as_ref().map(|t| t.get_root());
    let parsed = Parsed {
        functions: root.map_or(vec![], |r| {
            r.functions.iter().map(|f| f.name.text.clone()).collect()
        }),
        globals: root.map_or(vec![], |r| {
            r.globals.iter().map(|g| g.name.text.clone()).collect()
        }),
        imports: root.map_or(0, |r| r.imports.len()),
        fields: root.map_or(vec![], |r| {
            r.structs
                .iter()
                .flat_map(|s| s.fields.iter().map(|f| f.name.text.clone()))
                .collect()
        }),
        methods: root.map_or(vec![], |r| {
            r.structs
                .iter()
                .flat_map(|s| s.methods.iter().map(|m| m.name.text.clone()))
                .collect()
        }),
        variants: root.map_or(vec![], |r| {
            r.enums
                .iter()
                .flat_map(|e| e.variants.iter().map(|v| v.name.text.clone()))
                .collect()
        }),
        interface_methods: root.map_or(vec![], |r| {
            r.interfaces
                .iter()
                .flat_map(|i| i.methods.iter().map(|m| m.name.text.clone()))
                .collect()
        }),
        extend_methods: root.map_or(vec![], |r| {
            r.extends
                .iter()
                .flat_map(|e| e.methods.iter().map(|m| m.name.text.clone()))
                .collect()
        }),
        main_statements: root
            .and_then(|r| r.functions.iter().find(|f| f.name.text == "main"))
            .map_or(0, |f| f.body.len()),
        inactive: root.map_or(vec![], |r| {
            r.inactive_spans.iter().map(|s| (s.start, s.end)).collect()
        }),
        doc_trivia: root.is_some_and(|r| {
            r.functions
                .iter()
                .any(|f| !f.name.leading_trivia.is_empty())
        }),
        errors: vec![],
    };
    Parsed {
        errors: diagnostics
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect(),
        ..parsed
    }
}

fn windows_release() -> CfgSymbols {
    CfgSymbols::empty()
        .with(TargetSymbol::Release)
        .with(TargetSymbol::Native)
        .with(TargetSymbol::Windows)
}

fn linux_debug() -> CfgSymbols {
    CfgSymbols::empty()
        .with(TargetSymbol::Debug)
        .with(TargetSymbol::Native)
        .with(TargetSymbol::Linux)
}

fn browser() -> CfgSymbols {
    CfgSymbols::empty()
        .with(TargetSymbol::Debug)
        .with(TargetSymbol::Wasm)
        .with(TargetSymbol::Browser)
}

const PATHS: &str = r#"
@cfg(Target.Windows)
fun separator(): string {
    return "\\";
}

@cfg(!Target.Windows)
fun separator(): string {
    return "/";
}

fun main() {}
"#;

#[test]
fn opposite_conditions_keep_exactly_one_function() {
    let windows = parse_with(PATHS, windows_release());
    assert!(windows.errors.is_empty(), "{:?}", windows.errors);
    assert_eq!(windows.functions, ["separator", "main"]);
    assert_eq!(windows.inactive.len(), 1);

    let linux = parse_with(PATHS, linux_debug());
    assert_eq!(linux.functions, ["separator", "main"]);
    assert_eq!(linux.inactive.len(), 1);
    assert_ne!(windows.inactive, linux.inactive);
}

#[test]
fn inactive_span_covers_the_attribute_and_the_item() {
    let code = "@cfg(Target.Wasm)\nfun gone() {}\nfun main() {}\n";
    let parsed = parse_with(code, linux_debug());
    let end = code.find("fun main").unwrap() - 1;
    assert_eq!(parsed.inactive, [(0, end)]);
}

#[test]
fn operators_follow_not_and_or_precedence() {
    let code = r#"
@cfg(Target.Linux || Target.Windows && Target.Release)
fun a() {}
@cfg((Target.Linux || Target.Windows) && Target.Release)
fun b() {}
@cfg(!Target.Wasm && !(Target.Macos || Target.Ios))
fun c() {}
@cfg(!!Target.Debug)
fun d() {}
"#;
    let linux = parse_with(code, linux_debug());
    assert!(linux.errors.is_empty(), "{:?}", linux.errors);
    assert_eq!(linux.functions, ["a", "c", "d"]);
    let windows = parse_with(code, windows_release());
    assert_eq!(windows.functions, ["a", "b", "c"]);
    let web = parse_with(code, browser());
    assert_eq!(web.functions, ["d"]);
}

#[test]
fn cfg_gates_members_variants_and_statements() {
    let code = r#"
import system.io;
@cfg(Target.Browser)
import system.web;

let shared_value = 1;
@cfg(Target.Wasm)
let wasm_only = 2;

class Window {
    title: string;
    @cfg(Target.Windows)
    handle: long;

    @cfg(Target.Debug)
    fun dump() {}
    fun show() {}
}

enum Backend {
    Cpu,
    @cfg(Target.Windows)
    DirectX,
    @cfg(!Target.Windows)
    Vulkan,
}

interface Surface {
    fun draw(): void;
    @cfg(Target.Wasm)
    fun canvas(): void;
}

extend int {
    @cfg(Target.Release)
    fun fast(): int {
        return this;
    }
}

fun main() {
    let x = 1;
    @cfg(Target.Debug)
    print("debug build");
    @cfg(Target.Release)
    {
        print("release build");
    }
}
"#;
    let linux = parse_with(code, linux_debug());
    assert!(linux.errors.is_empty(), "{:?}", linux.errors);
    assert_eq!(linux.imports, 1);
    assert_eq!(linux.globals, ["shared_value"]);
    assert_eq!(linux.fields, ["title"]);
    assert_eq!(linux.methods, ["dump", "show"]);
    assert_eq!(linux.variants, ["Cpu", "Vulkan"]);
    assert_eq!(linux.interface_methods, ["draw"]);
    assert!(linux.extend_methods.is_empty());
    assert_eq!(linux.main_statements, 2);

    let windows = parse_with(code, windows_release());
    assert!(windows.errors.is_empty(), "{:?}", windows.errors);
    assert_eq!(windows.fields, ["title", "handle"]);
    assert_eq!(windows.methods, ["show"]);
    assert_eq!(windows.variants, ["Cpu", "DirectX"]);
    assert_eq!(windows.extend_methods, ["fast"]);
    assert_eq!(windows.main_statements, 2);

    let web = parse_with(code, browser());
    assert_eq!(web.imports, 2);
    assert_eq!(web.globals, ["shared_value", "wasm_only"]);
    assert_eq!(web.interface_methods, ["draw", "canvas"]);
}

#[test]
fn inactive_items_are_not_validated_beyond_syntax() {
    let code = "@cfg(Target.Windows)\ntype Handle = long;\nfun main() {}\n";
    let parsed = parse_with(code, linux_debug());
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(parsed.functions, ["main"]);
}

#[test]
fn doc_comments_above_cfg_stay_with_the_item() {
    let code = "/// Says hello.\n@cfg(Target.Linux)\nfun hello() {}\n";
    let parsed = parse_with(code, linux_debug());
    assert!(parsed.doc_trivia);
}

#[test]
fn cfg_errors_are_reported() {
    let cases = [
        ("@cfg(Target.Freebsd)\nfun f() {}\n", "unknown target 'Target.Freebsd'"),
        ("@cfg(Os.Linux)\nfun f() {}\n", "'@cfg' conditions test 'Target.<name>'"),
        ("@cfg\nfun f() {}\n", "'@cfg' needs a condition"),
        ("@cfg(Target.Linux Target.Wasm)\nfun f() {}\n", "expected '&&', '||', or ')'"),
        ("@cfg()\nfun f() {}\n", "expected a condition like 'Target.Windows'"),
        (
            "@cfg(Target.Linux)\n@cfg(Target.Debug)\nfun f() {}\n",
            "an item can have only one '@cfg'",
        ),
        (
            "@inline\n@cfg(Target.Linux)\nfun f() {}\n",
            "'@cfg' must be the first attribute",
        ),
        ("fun f() {\n    @cfg(Target.Linux)\n}\n", "'@cfg' must be followed by"),
    ];
    for (code, expected) in cases {
        let parsed = parse_with(code, linux_debug());
        assert!(
            parsed.errors.iter().any(|e| e.contains(expected)),
            "{code:?}: expected {expected:?} in {:?}",
            parsed.errors
        );
    }
}

#[test]
fn malformed_condition_keeps_the_item() {
    let parsed = parse_with("@cfg(Target.Nope)\nfun f() {}\n", linux_debug());
    assert_eq!(parsed.functions, ["f"]);
}

#[test]
fn symbol_key_is_stable_and_ordered() {
    assert_eq!(windows_release().key(), "Release,Windows,Native");
    assert_eq!(browser().key(), "Debug,Wasm,Browser");
    assert!(CfgSymbols::host_debug().defines(TargetSymbol::Debug));
}

#[test]
fn fuzz_cfg_conditions_never_panic() {
    let pieces = [
        "@", "cfg", "(", ")", "!", "&&", "||", "Target", ".", "Linux", "Wasm", "Nope", "fun",
        "f", "{", "}", ";", "let", "x", "=", "1", "class", "C", ":", "int", ",", "enum",
    ];
    let mut seed: u64 = 0x05ee_dcf9;
    for _ in 0..400 {
        let mut code = String::new();
        let len = (seed % 24) as usize + 1;
        for _ in 0..len {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            code.push_str(pieces[(seed >> 33) as usize % pieces.len()]);
            code.push(' ');
        }
        parse_with(&code, linux_debug());
        parse_with(&format!("fun main() {{ {code} }}"), browser());
    }
}

