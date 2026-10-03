mod common;

use common::{compile_test_pipeline, emit_ll_for, ir_func_body};
use dream_mir::backend::Target;

#[test]
fn native_locks_use_object_identity_while_wasm_uses_the_lock_word() {
    compile_test_pipeline(
        r#"
        shared class Counter {
            public value: int;
            public constructor() { this.value = 0; }
        }
        fun touch(counter: Counter): void { lock (counter) {} }
        fun main(): void { touch(Counter()); }
        "#,
        |hir, interner| {
            let mir = dream_mir::lower::lower_program(hir, interner);
            for target in [Target::native(), Target::wasm32()] {
                let ir = emit_ll_for(&mir, interner, target.clone());
                let body = ir_func_body(&ir, "touch");
                assert!(body.contains("@dream_lock_acquire("), "{}", body);
                assert!(body.contains("@dream_lock_release("), "{}", body);
                let adjusts_address = body.contains("add i64") || body.contains("add i32");
                assert_eq!(
                    adjusts_address,
                    target.spec().capabilities.linear_memory,
                    "{body}"
                );
            }
        },
    );
}
