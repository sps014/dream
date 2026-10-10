//! Function-signature registration (including overload/`main` validation and public-visibility
//! leakage checks) and the body-analysis / pending-instantiation fixpoint passes.

use super::*;
use crate::entry::{ENTRY_NAME, EntryTail, entry_tail_return};
use crate::function_table::FunctionTableInfo;
use dream_hir::{HExpr, HExprKind};
use dream_syntax::nodes::Type;

mod bodies;
mod entry;
mod visibility;
impl<'a> Analyzer<'a> {
    /// Pass 1: register every (non-generic) function signature; stash generic templates.
    pub(in crate::analyzer) fn register_functions(
        &mut self,
        node: &'a ProgramView<'a>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let mut exported = indexmap::IndexSet::new();
        for function in node.functions.iter() {
            diagnostics.file_path = file_path_string(&function.file_path);
            if dream_abi::attributes::has_export_attr(&function.attributes) {
                let name = &function.name.text;
                if !dream_abi::c_abi::is_c_identifier(name)
                    || name == "main"
                    || name.starts_with("dream_")
                    || name.starts_with("__")
                    || name.starts_with("_D")
                {
                    diagnostics.report_error(
                        format!("@export name '{name}' is reserved or is not a C identifier"),
                        Some(function.name.position),
                    );
                }
                if !exported.insert(name.clone()) {
                    diagnostics.report_error(
                        format!("duplicate @export symbol '{name}'"),
                        Some(function.name.position),
                    );
                }
                if function.generic_parameters.is_some()
                    || function.is_async
                    || function.parameters.iter().any(|p| p.is_variadic)
                {
                    diagnostics.report_error(
                        "@export functions must be synchronous, non-generic and non-variadic"
                            .to_string(),
                        Some(function.name.position),
                    );
                }
            }
            self.type_ctx
                .set_scope(self.graph.module_for_file(function.file_path.as_deref()));
            diagnostics.file_path = file_path_string(&function.file_path);
            self.check_reserved_name(&function.name, "function", diagnostics);
            if self.crate_type != CrateType::Lib && function.name.text == ENTRY_NAME {
                self.validate_entry_return_type(function, diagnostics);
            }
            if function.generic_parameters.is_some() {
                if dream_abi::attributes::has_test_attr(&function.attributes) {
                    diagnostics.report_error(
                        format!("@test function '{}' cannot be generic", function.name.text),
                        Some(function.name.position),
                    );
                }

                let def = self.type_ctx.register(
                    DefKind::Function,
                    &function.name.text,
                    generic_param_names(&function.generic_parameters),
                );
                self.generic_functions.insert(def, function);
                continue;
            }
            if function.visibility.is_public() {
                self.check_public_visibility(function, diagnostics);
            }
            let mut info = FunctionTableInfo::from(function, &mut self.type_ctx);
            self.function_table
                .record_declaration(function, info.identity.clone());
            self.record_ide_definition(
                info.identity.0,
                &function.name,
                function.file_path.as_deref(),
            );
            info.declaring_module = self.module_of(function.file_path.as_ref());
            if let Some(ret) = &function.return_type {
                self.check_type_not_static_class(ret, diagnostics);
            }
            for p in &function.parameters {
                self.check_type_not_static_class(&p.type_, diagnostics);
            }
            if dream_abi::attributes::has_test_attr(&function.attributes) {
                self.validate_test_function(function, diagnostics);
            }
            if function.is_extern && dream_abi::attributes::has_c_attr(&function.attributes) {
                self.validate_c_extern_signature(function, info.identity.0, diagnostics);
            }

            if let Err(e) =
                self.function_table
                    .add_overload(&function.name.text, info, &mut self.type_ctx)
            {
                diagnostics.report_error(e.to_string(), Some(function.name.position));
            }
        }
        // The entry point is exported under the fixed name `main`. It may be declared as `main()`
        // or `main(args: string[])`, but not overloaded or given any other signature.
        // A library has no process entry point, including in its imported modules.
        if self.crate_type == CrateType::Lib {
            for function in node.functions.iter().filter(|f| f.name.text == ENTRY_NAME) {
                diagnostics.file_path = file_path_string(&function.file_path);
                diagnostics.report_error(
                    "library crates must not declare a top-level 'main' (use --crate-type bin for runnable programs)".to_string(),
                    Some(function.name.position),
                );
            }
        } else if self.function_overloaded("main") {
            diagnostics.report_error("'main' cannot be overloaded".to_string(), None);
        } else if let Ok(info) = self.function_info("main") {
            let string_array = self
                .type_ctx
                .interner
                .array(self.type_ctx.interner.string());
            let ok = info.parameters.is_empty() || info.parameters == [string_array];
            if !ok {
                diagnostics.report_error(
                    "'main' must be declared as 'main()' or 'main(args: string[])'".to_string(),
                    None,
                );
            }
        }
    }

    fn validate_test_function(&self, function: &FunctionNode<'a>, diagnostics: &mut DiagnosticBag) {
        if function.name.text == "main" {
            diagnostics.report_error(
                "'main' cannot be marked @test".to_string(),
                Some(function.name.position),
            );
        }
        if function.is_async {
            diagnostics.report_error(
                format!("@test function '{}' cannot be async", function.name.text),
                Some(function.name.position),
            );
        }
        if function.is_extern {
            diagnostics.report_error(
                format!("@test function '{}' cannot be extern", function.name.text),
                Some(function.name.position),
            );
        }
        if !function.parameters.is_empty() {
            diagnostics.report_error(
                format!(
                    "@test function '{}' must take no parameters",
                    function.name.text
                ),
                Some(function.name.position),
            );
        }
        let ret = function
            .return_type
            .as_ref()
            .map(|t| t.get_type())
            .unwrap_or_else(|| "void".to_string());
        if ret != "void" {
            diagnostics.report_error(
                format!("@test function '{}' must return void", function.name.text),
                Some(function.name.position),
            );
        }
        if dream_abi::attributes::has_generator_attr(&function.attributes) {
            diagnostics.report_error(
                format!(
                    "@test function '{}' cannot also be a generator",
                    function.name.text
                ),
                Some(function.name.position),
            );
        }
    }
}

/// Healthy programs monomorphize a few thousand methods with short mangled names. A generic
/// whose field types amplify under substitution (e.g. a `List<fun(T): bool>` field on
/// `class C<T>` while something derives `C<fun(T): bool>`) diverges: every expansion round
/// wraps the type again, so mangled names grow without limit while breadth explodes. Either
/// signature trips these bounds and turns an infinite loop into an immediate diagnostic.
const MAX_MANGLED_NAME_LEN: usize = 512;
const MAX_INSTANTIATION_ITEMS: usize = 50_000;

fn check_instantiation_bounds(
    max_len: &mut usize,
    items: &mut usize,
    name: &str,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), SemanticError> {
    let diverging = *items > MAX_INSTANTIATION_ITEMS;
    if name.len() > *max_len {
        *max_len = name.len();
    }
    if !diverging && *max_len <= MAX_MANGLED_NAME_LEN {
        return Ok(());
    }
    diagnostics.report_error(
        format!(
            "generic instantiation grew too deep while expanding '{}...': monomorphization is diverging, usually because a generic class has a field that wraps its own type parameter (e.g. `ps: List<fun(T): bool>` on `class C<T>`) while something also derives `C<fun(T): bool>` from it; restructure the class so its field types do not nest the parameter deeper",
            &name[..name.len().min(120)]
        ),
        None,
    );
    Err(SemanticError::AnalysisFailed)
}
