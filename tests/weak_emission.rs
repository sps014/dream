mod common;

use common::{emit_hir_to_module_rc, ir_func_body, SYSTEM_STUB};

#[test]
fn typed_drops_claim_counts_and_clear_weak_slots_before_destructor_revival() {
    let source = format!(
        "{}\n{}",
        SYSTEM_STUB,
        r#"
        class Item {
            public value: int;
            public constructor(value: int) { this.value = value; }
            del() { System.println(this.value); }
        }
        class Holder {
            public item: Item;
            public constructor(item: Item) { this.item = item; }
        }
        fun main(): void {
            let holder = Holder(Item(42));
            System.println(holder.item.value);
        }
        "#
    );
    let ir = emit_hir_to_module_rc(&source);
    let holder = ir_func_body(&ir, "destroy_Holder");
    assert!(holder.contains("@dream_rc_claim_unique("), "{}", holder);
    assert!(!ir.contains("@dream_rc_one("));
    for name in ["release_Item_into", "destroy_Item"] {
        let body = ir_func_body(&ir, name);
        let clear = body
            .find("@dream_weak_prepare_destroy(")
            .expect("weak clear");
        let revive = body.find("@dream_rc_revive(").expect("destructor revival");
        assert!(clear < revive, "{}", body);
    }
}
