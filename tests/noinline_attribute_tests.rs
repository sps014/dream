use crate::common;

#[test]
fn only_the_attribute_prevents_inlining() {
    let ir = common::emit_hir_to_module_optimized(
        "@noinline
        fun keep(x: int): int { return x + 1; }
        fun kitchenSink(x: int): int { return x + 2; }
        @noinline
        fun run(n: int): int { return keep(n) + kitchenSink(n); }
        fun main(): int { return run(3); }",
    );
    let body = common::ir_func_body(&ir, "run");
    assert!(!body.is_empty(), "missing run body:\n{}", ir);
    assert!(body.contains("call i32 @keep("), "{}", body);
    assert!(!body.contains("@kitchenSink("), "{}", body);
}
