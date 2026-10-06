use crate::common;

use common::{compile_test_pipeline, emit_ll, ir_func_body, SYSTEM_STUB};

#[test]
fn destructor_identity_survives_symbol_changes_and_ignores_decoy_names() {
    let source = format!(
        "{}\n{}",
        SYSTEM_STUB,
        r#"
        class Item {
            public value: int;
            public constructor(value: int) { this.value = value; }
            del() { System.println(this.value); }
        }
        class Plain {
            public value: int;
            public constructor(value: int) { this.value = value; }
        }
        fun Plain_del(item: Plain): void { System.println(item.value); }
        fun main(): void {
            let item = Item(42);
            let plain = Plain(7);
            System.println(item.value + plain.value);
        }
    "#
    );
    compile_test_pipeline(&source, |hir, interner| {
        let item = hir
            .layouts
            .structs
            .values()
            .find(|l| l.name == "Item")
            .expect("Item layout");
        let destructor = item.destructor.expect("resolved Item destructor");
        let plain = hir
            .layouts
            .structs
            .values()
            .find(|l| l.name == "Plain")
            .expect("Plain layout");
        assert!(!plain.has_destructor());
        let mut mir = dream_mir::lower::lower_program(hir, interner);
        let cleanup = mir
            .functions
            .iter_mut()
            .find(|f| f.def == destructor)
            .expect("destructor function");
        cleanup.name = "renamed_cleanup".into();
        cleanup.symbol = "renamed_cleanup".into();
        dream_mir::passes::optimize_module(&mut mir, interner);
        assert!(
            mir.functions.iter().any(|f| f.def == destructor),
            "pruning must follow DefId"
        );
        let ir = emit_ll(&mir, interner);
        assert!(ir_func_body(&ir, "destroy_Item").contains("@renamed_cleanup("));
        assert!(!ir_func_body(&ir, "destroy_Plain").contains("@Plain_del("));
    });
}
