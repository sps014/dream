//! Per-kernel emit context: scopes, binding lookup, identifier rewriting.

use super::ident::escape_wgsl_ident;
use super::types::GpuBinding;
use dream_diagnostics::DiagnosticBag;
use dream_text::text_span::TextSpan;
use indexmap::IndexMap;
use std::cell::RefCell;

/// Module-scope WGSL names are prefixed with `entry` so joined multi-kernel modules do not
/// redeclare the same storage/uniform/workgroup globals.
pub(super) struct EmitCtx<'a> {
    /// Unique entry name (`dream_<fn>`), used as the prefix for module-scope identifiers.
    pub(super) prefix: &'a str,
    pub(super) bindings: &'a [GpuBinding],
    /// Dream names of `@workgroup` locals declared in this kernel (rewritten with `prefix_`).
    pub(super) workgroup_names: &'a [String],
    /// Nested scopes of `var` locals (innermost last). Locals shadow bindings of the same name.
    pub(super) scopes: RefCell<Vec<IndexMap<String, String>>>,
    /// Struct name → field name → WGSL type (for varying/vertex attribute member inference).
    pub(super) struct_fields: &'a IndexMap<String, IndexMap<String, String>>,
    /// `@gpu` helper name → WGSL return type (for unannotated `let` inference).
    pub(super) helper_returns: &'a IndexMap<String, String>,
    /// Dream function name, used in GPU diagnostic messages.
    pub(super) kernel: &'a str,
    pub(super) diagnostics: RefCell<&'a mut DiagnosticBag>,
}

impl EmitCtx<'_> {
    pub(super) fn report_error(&self, message: String, span: Option<TextSpan>) {
        self.diagnostics.borrow_mut().report_error(message, span);
    }

    pub(super) fn unsupported_expr(&self, kind: &str, span: Option<TextSpan>) -> String {
        self.report_error(
            format!(
                "GPU shader '{}' contains unsupported {kind}; remove it or rewrite with supported expressions",
                self.kernel
            ),
            span,
        );
        "0".into()
    }

    pub(super) fn mangle(&self, dream_name: &str) -> String {
        format!("{}_{}", self.prefix, dream_name)
    }

    pub(super) fn uniforms_var(&self) -> String {
        format!("dream_uniforms_{}", self.prefix)
    }

    pub(super) fn push_scope(&self) {
        self.scopes.borrow_mut().push(IndexMap::new());
    }

    pub(super) fn pop_scope(&self) {
        self.scopes.borrow_mut().pop();
    }

    pub(super) fn define_local(&self, name: &str, wgsl_ty: String) {
        if let Some(scope) = self.scopes.borrow_mut().last_mut() {
            scope.insert(name.to_string(), wgsl_ty);
        }
    }

    pub(super) fn lookup_local(&self, name: &str) -> Option<String> {
        let scopes = self.scopes.borrow();
        for scope in scopes.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t.clone());
            }
        }
        None
    }

    pub(super) fn is_local(&self, name: &str) -> bool {
        self.lookup_local(name).is_some()
    }

    pub(super) fn binding(&self, name: &str) -> Option<&GpuBinding> {
        self.bindings.iter().find(|b| b.name == name)
    }

    pub(super) fn is_uniform(&self, name: &str) -> bool {
        !self.is_local(name)
            && self
                .bindings
                .iter()
                .any(|b| b.kind == "uniform" && b.name == name)
    }

    pub(super) fn is_resource(&self, name: &str) -> bool {
        !self.is_local(name)
            && self.bindings.iter().any(|b| {
                matches!(
                    b.kind,
                    "storage" | "texture" | "storage_texture" | "sampler" | "texture_cube"
                ) && b.name == name
            })
    }

    pub(super) fn is_atomic_buf(&self, name: &str) -> bool {
        !self.is_local(name)
            && self
                .bindings
                .iter()
                .any(|b| b.kind == "storage" && b.atomic && b.name == name)
    }

    pub(super) fn is_workgroup(&self, name: &str) -> bool {
        !self.is_local(name) && self.workgroup_names.iter().any(|n| n == name)
    }

    /// Map a Dream identifier to WGSL. Locals always win over bindings (shadowing).
    pub(super) fn rewrite_ident(&self, name: &str) -> String {
        if self.is_local(name) {
            escape_wgsl_ident(name)
        } else if self.is_uniform(name) {
            format!("{}.{}", self.uniforms_var(), escape_wgsl_ident(name))
        } else if self.is_resource(name) || self.is_workgroup(name) {
            self.mangle(name)
        } else {
            escape_wgsl_ident(name)
        }
    }

    pub(super) fn lookup_struct_field(&self, struct_ty: &str, field: &str) -> Option<String> {
        self.struct_fields
            .get(struct_ty)
            .and_then(|fields| fields.get(field).cloned())
    }
}
