use super::super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{FunctionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    /// Type-checks a constructor call `Struct(args)`. When the struct defines a custom `constructor`
    /// the call is checked against `init`'s parameters; otherwise the class has an implicit zero-arg
    /// default constructor(`Struct()`) that leaves every field at its zero value. A struct may
    /// declare more than one `constructor` overload (registered like any other method via
    /// `add_overload`); the matching one is selected by argument count/types exactly like an
    /// overloaded free function or method call. Returns the selected constructor's *emitted*
    /// (possibly signature-mangled) name alongside the constructed type, so the caller resolves the
    /// `New` HIR node's `DefId` to the actual overload chosen here rather than re-deriving the bare
    /// `{struct}_constructor` name (which is ambiguous once there is more than one overload).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn analyze_constructor_call(
        &mut self,
        name: &SyntaxToken,
        generic_args: &Option<Vec<Type>>,
        params_types: &mut Vec<dream_types::TypeId>,
        arg_hirs: &mut Vec<Option<dream_hir::HExpr>>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(Type, Option<crate::function_table::FunctionIdentity>), SemanticError> {
        if let Some(args) = generic_args { self.ensure_struct_instantiated(&name.text, args, &name.position, diagnostics); }
        let result = Type::Struct(name.clone(), generic_args.clone());
        let owner = self.type_ctx.lower(&result);
        let struct_name = self.type_id_display(owner);
        let ctor_member = dream_syntax::nodes::types::CONSTRUCTOR_NAME;
        if generic_args.is_none() && self.type_ctx.resolve(DefKind::Struct, &name.text).is_some_and(|def| self.generic_struct(def).is_some()) {
            diagnostics.report_error(format!("Generic class '{}' requires type arguments", name.text), Some(name.position));
        }

        // File/module-level visibility (Axis 2): a non-public class is only constructible from its
        // own file.
        if let Some(info) = self.struct_info(owner) {
            if !self.visible_across_files(
                &info.file_path,
                info.visibility,
                parent_function.file_path.as_ref(),
            ) {
                let decl_file = info.file_path.clone();
                self.report_not_public("Class", &name.text, &decl_file, name.position, diagnostics);
            }
        }

        // A struct with more than one `constructor` overload is resolved exactly like an
        // overloaded free function/method: the implicit `this` (the struct itself) plus the given
        // argument types are matched against every registered overload's full parameter list.
        let resolved_ctor: Option<crate::function_table::FunctionTableInfo> =
            if self.method_overloaded(owner, ctor_member) {
                let mut selection_args = Vec::with_capacity(params_types.len() + 1);
                selection_args.push(owner);
                selection_args.extend(params_types.iter().cloned());
                match self.select_method_overload(owner, ctor_member, &selection_args) {
                    Ok(sig) => Some(sig),
                    Err(message) => {
                        diagnostics.report_error(message, Some(name.position));
                        None
                    }
                }
            } else {
                self.method_info(owner, ctor_member).ok()
            };

        // Class-member visibility (Axis 2): a private constructor is only callable from the
        // declaring type's methods; `internal` from the same module; `public` everywhere the type
        // is reachable. An implicit zero-arg default (no registered `constructor`) is public.
        if let Some(sig) = &resolved_ctor {
            let (base_name, _) =
                Self::resolve_struct_parts(&Type::Struct(name.clone(), generic_args.clone()))
                    .unwrap_or_else(|| (struct_name.clone(), vec![]));
            let struct_file = self
                .struct_info(owner)
                .and_then(|info| info.file_path.clone())
                .or_else(|| sig.declaring_file.clone());
            if !self.member_accessible(
                sig.visibility,
                &struct_file,
                parent_function.file_path.as_ref(),
                self.in_methods_of(parent_function, owner),
            ) {
                diagnostics.report_error(
                    format!("constructor of '{}' is not accessible here", base_name),
                    Some(name.position),
                );
            }
        }

        // `expected` are the constructor's parameter types (a user `constructor` skips its implicit
        // `this`); `expected_defaults` are the parallel default values. A class with no explicit
        // `constructor` has an implicit zero-arg default constructor, so it expects no arguments.
        // An overloaded constructor whose resolution already failed above skips the redundant
        // arity/type re-check below (its own error was already reported).
        let overload_resolution_failed =
            resolved_ctor.is_none() && self.method_overloaded(owner, ctor_member);
        let (expected, expected_defaults, expected_param_tys): (
            Vec<dream_types::TypeId>,
            Vec<Option<Type>>,
            Vec<Type>,
        ) = match &resolved_ctor {
            // `constructor` is registered as a method, so parameter 0 is the implicit `this`.
            Some(sig) => (
                sig.parameters.iter().skip(1).cloned().collect(),
                sig.defaults.iter().skip(1).cloned().collect(),
                sig.parameter_types.iter().skip(1).cloned().collect(),
            ),
            None => (Vec::new(), Vec::new(), Vec::new()),
        };

        if !overload_resolution_failed {
            let total = expected.len();
            let required = Self::required_arg_count(&expected_defaults, total);
            let given = params_types.len();
            if given < required || given > total {
                let message = if required == total {
                    format!(
                        "Constructor for '{}' expects {} argument(s), but {} were given",
                        struct_name, total, given
                    )
                } else {
                    format!(
                        "Constructor for '{}' expects between {} and {} argument(s), but {} were given",
                        struct_name, required, total, given
                    )
                };
                diagnostics.report_error(message, Some(name.position));
            } else {
                // Fill omitted trailing arguments with their defaults (extends both the type list and
                // the emitted argument HIR so the generated `New` receives the complete argument set).
                self.substitute_default_args(
                    (&expected_defaults, &expected_param_tys),
                    params_types,
                    arg_hirs,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                self.validate_arguments(
                    &format!("Constructor for '{}'", struct_name),
                    &expected,
                    params_types,
                    name.position,
                    diagnostics,
                );
            }
        }

        Ok((
            result,
            resolved_ctor.map(|sig| sig.identity),
        ))
    }
}
