use crate::common;

use common::{emit_hir_to_ir, ir_func_body};

#[test]
fn managed_field_and_array_stores_publish_before_installing_the_child() {
    let (ir, _) = emit_hir_to_ir(
        "class Node {} class Holder { public next: Node; }
         fun field(n: Holder, child: Node): void { n.next = child; }
         fun element(a: Node[], child: Node): void { a[0] = child; }",
    );
    for name in ["field", "element"] {
        let body = ir_func_body(&ir, name);
        let barrier = body.find("@dream_publish_child(").expect(body);
        assert!(body[barrier..].contains("store ptr"), "{}", body);
        assert!(body[..barrier].contains("load ptr"), "{}", body);
    }
}

#[test]
fn inline_value_stores_publish_nested_references_and_only_the_active_union_arm() {
    let (ir, _) = emit_hir_to_ir(
        "class Node {}
         struct Wrap { public node: Node; }
         enum struct Payload { Left(value: Wrap), Right(value: Node) }
         class Holder { public wrap: Wrap; public payload: Payload; }
         fun wrap(h: Holder, w: Wrap): void { h.wrap = w; }
         fun payload(h: Holder, p: Payload): void { h.payload = p; }
         fun array(a: Wrap[], w: Wrap): void { a[0] = w; }",
    );
    for name in ["wrap", "payload", "array"] {
        let body = ir_func_body(&ir, name);
        let barrier = body.find("@dream_publish_child(").expect(body);
        assert!(body[barrier..].contains("@llvm.memcpy"), "{}", body);
    }
    assert!(ir_func_body(&ir, "payload").contains("switch i32"));
}

#[test]
fn value_constructor_ref_interiors_never_read_a_heap_header() {
    let (ir, _) = emit_hir_to_ir(
        "class Node {}
         struct Wrap { public node: Node;
             public constructor(node: Node) { this.node = node; }
         }",
    );
    assert!(
        ir.lines()
            .any(|line| line.contains("call void @dream_publish_child(ptr null,")),
        "{}",
        ir
    );
    let body = ir_func_body(&ir, "s0_4_Wrap_0_constructor");
    assert!(!body.contains("inttoptr"), "{}", body);
}
