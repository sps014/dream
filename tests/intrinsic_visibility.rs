use crate::common::analyze_code;

#[test]
fn private_generic_intrinsic_is_rejected_before_special_dispatch() {
    let diagnostics = analyze_code(
        r#"
static class Hidden {
    @intrinsic("array_new")
    static extern fun alloc<T>(n: int): T[];
}
fun main(): void { let values = Hidden.alloc<int>(1); }
"#,
    );
    assert!(diagnostics.has_errors());
    assert!(
        diagnostics
            .errors()
            .any(|d| d.message.contains("private to"))
    );
}

#[test]
fn private_plain_intrinsic_is_not_implicitly_public() {
    let diagnostics = analyze_code(
        r#"
static class Hidden {
    @intrinsic("debug_get_live_objects")
    static extern fun count(): int;
}
fun main(): void { let count = Hidden.count(); }
"#,
    );
    assert!(diagnostics.has_errors());
}

#[test]
fn private_intrinsic_is_callable_inside_its_declaring_type() {
    let diagnostics = analyze_code(
        r#"
static class Hidden {
    @intrinsic("array_new")
    static extern fun alloc<T>(n: int): T[];
    public static fun make(): int[] { return Hidden.alloc<int>(1); }
}
fun main(): void { let values = Hidden.make(); }
"#,
    );
    assert!(!diagnostics.has_errors());
}

#[test]
fn internal_intrinsic_is_callable_in_its_module() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.dream");
    let out = dir.path().join("main.ll");
    std::fs::write(
        &source,
        r#"
module trusted;
import system;
static class Hidden {
    @intrinsic("array_new")
    internal static extern fun alloc<T>(n: int): T[];
}
fun main(): void { let values = Hidden.alloc<int>(1); }
"#,
    )
    .unwrap();
    dream::driver::compiler::Compiler::new(dream_mir::backend::Target::native())
        .compile(&source.display().to_string(), out.to_str().unwrap())
        .unwrap();
}

#[test]
fn internal_intrinsic_rejects_a_different_module() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("helper.dream"),
        r#"
module trusted;
import system;
public static class Hidden {
    @intrinsic("array_new")
    internal static extern fun alloc<T>(n: int): T[];
}
"#,
    )
    .unwrap();
    let source = dir.path().join("main.dream");
    std::fs::write(&source, "module other; import system; import helper; fun main(): void { let x = Hidden.alloc<int>(1); }").unwrap();
    let result = dream::driver::compiler::Compiler::new(dream_mir::backend::Target::native())
        .with_render_diagnostics(false)
        .compile(
            &source.display().to_string(),
            dir.path().join("main.ll").to_str().unwrap(),
        );
    assert!(result.is_err());
}

#[test]
fn public_intrinsic_is_callable_and_remains_an_import() {
    let diagnostics = analyze_code(
        r#"
static class Visible {
    @intrinsic("array_new")
    public static extern fun alloc<T>(n: int): T[];
}
fun main(): void { let values = Visible.alloc<int>(1); }
"#,
    );
    assert!(!diagnostics.has_errors());
}

#[test]
fn cancellation_state_intrinsics_are_reserved_for_stdlib_ownership() {
    let diagnostics = analyze_code(
        r#"
static class Hidden {
    @intrinsic("dream_cancellation_unregister")
    static extern fun release(state: usize): void;
}
fun main(): void { Hidden.release((usize)0); }
"#,
    );
    assert!(diagnostics.errors().any(|d| {
        d.message
            .contains("cancellation state intrinsics are reserved")
    }));
}
