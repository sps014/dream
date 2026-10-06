//! Escape rules for `ref struct`s: a stack-only value may not reach the heap through a field, a
//! generic argument, an array element, a union payload, or an `async` frame.

use super::*;
use dream_syntax::nodes::struct_node::StructFieldNode;

impl<'a> Analyzer<'a> {
    /// Reports an error if `field`'s type is a `ref struct` — such a type cannot be stored as a
    /// field of a `class` or ordinary `struct`, since that would let a stack-only value outlive
    /// the stack frame it was created in.
    pub(in crate::analyzer) fn reject_ref_struct_field(
        &mut self,
        owner_name: &str,
        field: &StructFieldNode,
        diagnostics: &mut DiagnosticBag,
    ) {
        let tid = self.type_ctx.lower(&field.field_type);
        if self.type_ctx.interner.is_ref_struct_type(tid) {
            diagnostics.report_error(
                format!(
                    "field '{}' of '{}' cannot have type '{}': a 'ref struct' cannot be stored as a field (it would let a stack-only value escape its stack frame)",
                    field.name.text,
                    owner_name,
                    self.ty_display(&field.field_type)
                ),
                Some(field.name.position),
            );
        }
    }

    /// Rejects a `ref struct`-typed parameter on any `async` function, method, or `extend`-block
    /// method in the program: an `async` call may suspend at an `await`, which spills the coroutine's
    /// live locals (including its parameters) into a heap-allocated state object so they survive
    /// across the suspend point — exactly the kind of stack-frame escape a `ref struct` forbids.
    /// Generic templates are checked once per instantiation's concrete parameter types would be
    /// ideal, but templates don't carry a `ref struct` argument until monomorphized, so this walks
    /// only concrete (non-generic) declarations, matching this analysis's stated conservative scope.
    pub(in crate::analyzer) fn check_ref_struct_async_boundary(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let check_fn = |this: &mut Self,
                        f: &dream_syntax::nodes::function::FunctionNode<'a>,
                        diags: &mut DiagnosticBag| {
            if !f.is_async {
                return;
            }
            for p in &f.parameters {
                let tid = this.type_ctx.lower(&p.type_);
                if this.type_ctx.interner.is_ref_struct_type(tid) {
                    diags.report_error(
                        format!(
                            "async function '{}' cannot take 'ref struct' parameter '{}' of type '{}': it may need to survive an 'await' suspend point, which would spill it into the heap-allocated coroutine state",
                            f.name.text,
                            p.name.text,
                            this.ty_display(&p.type_)
                        ),
                        Some(f.name.position),
                    );
                }
            }
        };
        for f in node.functions.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(f.file_path.as_deref()));
            check_fn(self, f, diagnostics);
        }
        for s in node.structs.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(s.file_path.as_deref()));
            for m in &s.methods {
                check_fn(self, m, diagnostics);
            }
        }
        for e in node.extends.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(e.file_path.as_deref()));
            for m in &e.methods {
                check_fn(self, m, diagnostics);
            }
        }
        for en in node.enums.iter() {
            self.type_ctx
                .set_scope(self.graph.module_for_file(en.file_path.as_deref()));
            for m in &en.methods {
                check_fn(self, m, diagnostics);
            }
        }
    }

    /// [`Self::reject_ref_struct_type_args`] for a generic function or method instantiation.
    pub(in crate::analyzer) fn reject_ref_struct_bindings(
        &mut self,
        bindings: &crate::analyzer::GenericBindings,
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        let args: Vec<Type> = bindings.values().cloned().collect();
        self.reject_ref_struct_type_args(&args, position, diagnostics);
    }

    /// Rejects any `ref struct` type appearing in `args` as a generic type argument: instantiating
    /// a generic class/struct/union/function with a `ref struct` argument would store it in a field,
    /// array element, or heap payload somewhere in that generic's body, letting a stack-only value
    /// escape its frame. Called at every generic instantiation site (classes, unions, generic
    /// functions and methods). Returns true when some argument was rejected; the caller mutes
    /// further reports while instantiating that generic's body.
    pub(in crate::analyzer) fn reject_ref_struct_type_args(
        &mut self,
        args: &[Type],
        position: &TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) -> bool {
        let mut rejected = false;
        for arg in args {
            let tid = self.type_ctx.lower(arg);
            if self.type_ctx.interner.is_ref_struct_type(tid) {
                rejected = true;
                if self.ref_struct_escape_muted {
                    continue;
                }
                diagnostics.report_error(
                    format!(
                        "'{}' is a 'ref struct' and cannot be used as a generic type argument (it would be stored in a heap-allocated container, letting it escape its stack frame)",
                        self.ty_display(arg)
                    ),
                    Some(*position),
                );
            }
        }
        rejected
    }

    /// Rejects building a `T[]` whose element type is a `ref struct`: arrays live on the heap.
    /// Array values only come from literals, `[v; n]`, and the `Buffer` intrinsics, so checking
    /// those creation sites covers every array of such a type.
    pub(in crate::analyzer) fn reject_ref_struct_array_element(
        &mut self,
        element: &Type,
        position: Option<TextSpan>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let tid = self.type_ctx.lower(element);
        if self.ref_struct_escape_muted || !self.type_ctx.interner.is_ref_struct_type(tid) {
            return;
        }
        diagnostics.report_error(
            format!(
                "'{}' is a 'ref struct' and cannot be an array element type (arrays are heap-allocated, letting it escape its stack frame)",
                self.ty_display(element)
            ),
            position,
        );
    }
}
