mod common;

use common::{
    compile_test_pipeline_for, emit_hir_to_module, emit_ll_for, ir_func_body, CLOSURE_STUB,
};
use dream_mir::backend::Target;

fn reference_path_has_no_integer_round_trip(ir: &str, function: &str) {
    let body = ir_func_body(ir, function);
    assert!(!body.is_empty(), "missing {}", function);
    assert!(!body.contains("inttoptr"), "{}:\n{}", function, body);
    assert!(!body.contains("ptrtoint"), "{}:\n{}", function, body);
    assert!(
        !body.contains("getelementptr inbounds"),
        "{}:\n{}",
        function,
        body
    );
}

#[test]
fn native_aliasing_fields_use_pointer_values_and_plain_gep() {
    let ir = emit_hir_to_module(
        r#"
        class Cell { public value: int; public label: string; }
        fun alias(a: Cell, b: Cell): int {
            a.value = a.value + 1;
            return b.value;
        }
        fun label(cell: Cell): string { return cell.label; }
        fun set_label(cell: Cell, value: string): void { cell.label = value; }
        fun identity(cell: Cell): Cell { return cell; }
        "#,
    );
    assert!(ir.contains("@alias(ptr %a0, ptr %a1)"), "{}", ir);
    assert!(
        ir.contains("define internal ptr @identity(ptr %a0)"),
        "{}",
        ir
    );
    for name in ["alias", "label", "set_label", "identity"] {
        reference_path_has_no_integer_round_trip(&ir, name);
    }
    let get = ir_func_body(&ir, "label");
    let set = ir_func_body(&ir, "set_label");
    assert!(get.contains("getelementptr i8, ptr"), "{}", get);
    assert!(get.contains("load ptr,"), "{}", get);
    assert!(set.contains("store ptr "), "{}", set);
    let signature = ir
        .lines()
        .find(|line| line.starts_with("define ") && line.contains("@alias("))
        .unwrap();
    assert!(
        !signature.contains("noalias"),
        "pointer representation is not exclusivity"
    );
}

#[test]
fn inline_reference_fields_and_nullable_references_keep_pointer_storage() {
    let ir = emit_hir_to_module(
        r#"
        enum Option<T> { Some(T), None }
        class Cell { public value: int; }
        struct Pair { public label: string; public cell: Cell; }
        fun label(pair: Pair): string { return pair.label; }
        fun optional(cell: Option<Cell>): Option<Cell> { return cell; }
        fun value(cell: Option<Cell>): int {
            switch(cell) {
                Some(c) => { return c.value; }
                None => { return 0; }
            }
        }
        "#,
    );
    assert!(
        ir.contains("define internal ptr @optional(ptr %a0)"),
        "{}",
        ir
    );
    let label = ir_func_body(&ir, "label");
    assert!(label.contains("load ptr,"), "{}", label);
    let value = ir_func_body(&ir, "value");
    assert!(
        value.contains("icmp eq ptr") || value.contains("icmp ne ptr"),
        "{}",
        value
    );
    assert!(value.contains("null"), "{}", value);
    for name in ["label", "optional", "value"] {
        reference_path_has_no_integer_round_trip(&ir, name);
    }
}

#[test]
fn closure_environment_is_pointer_but_function_selector_is_integer() {
    let source = format!(
        r#"{CLOSURE_STUB}
        fun make_adder(n: int): fun(int): int {{ return (x) => x + n; }}
        fun invoke(n: int): int {{ let f = make_adder(n); return f(2); }}
        "#,
    );
    let ir = emit_hir_to_module(&source);
    assert!(
        ir.contains("define internal ptr @make_adder(i32 %a0)"),
        "{}",
        ir
    );
    assert!(ir.contains("call ptr @dream_funcbox_env(ptr "), "{}", ir);
    assert!(
        ir.contains("call i32 @dream_funcbox_funcidx(ptr "),
        "{}",
        ir
    );
    assert!(
        ir.contains("store ptr ") && ir.contains("ptr @g0"),
        "{}",
        ir
    );
    reference_path_has_no_integer_round_trip(&ir, "make_adder");
    reference_path_has_no_integer_round_trip(&ir, "invoke");
}

#[test]
fn async_future_and_frame_references_have_pointer_signatures() {
    let ir = emit_hir_to_module(
        r#"
        async fun child(value: string): string { return value; }
        async fun parent(value: string): string { return child(value).await; }
        "#,
    );
    assert!(
        ir.contains("define internal ptr @parent(ptr %a0)"),
        "{}",
        ir
    );
    assert!(
        ir.contains("define internal i32 @poll_parent(ptr %a0)"),
        "{}",
        ir
    );
    let stub = ir_func_body(&ir, "parent");
    assert!(stub.contains("store ptr "), "{}", stub);
    reference_path_has_no_integer_round_trip(&ir, "parent");
    // Poll completion deliberately transports primitive/reference results through the
    // scheduler's tagged payload word; that boundary is not an ordinary field access.
}

#[test]
fn wasm_reference_boundary_remains_a_linear_memory_offset() {
    for target in [Target::native(), Target::wasm32()] {
        compile_test_pipeline_for(
            "fun first(values: int[]): int { return values[0]; }",
            target.clone(),
            |hir, types| {
                let mir = dream_mir::lower::lower_program(hir, types);
                let ir = emit_ll_for(&mir, types, target.clone());
                let reference = if target.spec().capabilities.linear_memory {
                    "i32"
                } else {
                    "ptr"
                };
                assert!(ir.contains(&format!("@first({reference} %a0)")), "{}", ir);
                if !target.spec().capabilities.linear_memory {
                    reference_path_has_no_integer_round_trip(&ir, "first");
                }
            },
        );
    }
}
