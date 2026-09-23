//! Identifier resolution (locals, globals, first-class function values) and the name→`Type` parser.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{FunctionNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_identifier(
        &mut self,
        id: &SyntaxToken,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        if id.text == "_" {
            diagnostics.report_error(
                "'_' is a discard and cannot be used as a value".to_string(),
                Some(id.position),
            );
            self.hir_fail();
            return Ok(Type::Unknown);
        }
        self.check_local_not_moved(&id.text, Some(id.position), diagnostics);
        let lookup = (*symbol_table).as_ref().borrow().get_symbol(id);
        let r = match lookup {
            Ok(t) => {
                (*symbol_table).as_ref().borrow_mut().mark_used(&id.text);
                // A local bound to a polymorphic generic function item instantiates when the
                // use site publishes a concrete `fun(...)` expected type.
                if let Type::GenericFunctionItem(ref gname) = t {
                    if matches!(
                        self.current_expected_type
                            .as_ref()
                            .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings)),
                        Some(Type::Function(_, _))
                    ) {
                        let tok = synthetic_token(TokenKind::IdentifierToken, gname);
                        return match self.instantiate_generic_function_value(&tok, diagnostics) {
                            Some(func_ty) => Ok(func_ty),
                            None => Ok(Type::Unknown),
                        };
                    }
                }
                t
            }
            Err(e) => {
                // A bare identifier that names a top-level function is a first-class function value.
                if self.function_table.get_function(&id.text).is_ok() {
                    return Ok(self.function_value(id, &id.text));
                }
                if self.function_table.is_overloaded(&id.text) {
                    return Ok(self.overloaded_function_value(id, diagnostics));
                }
                // A generic function used as a value: with a `fun(...)` context, instantiate now;
                // otherwise bind a polymorphic item that instantiates at each later use.
                if self.generic_functions.contains_key(&id.text) {
                    let expected = self
                        .current_expected_type
                        .as_ref()
                        .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings));
                    if matches!(expected, Some(Type::Function(_, _))) {
                        return match self.instantiate_generic_function_value(id, diagnostics) {
                            Some(func_ty) => Ok(func_ty),
                            None => Ok(Type::Unknown),
                        };
                    }
                    self.hir_none();
                    return Ok(Type::GenericFunctionItem(id.text.clone()));
                }
                if let Some(expected) = self.current_expected_type.clone() {
                    let enum_name = match &expected {
                        Type::Struct(tok, _) => tok.text.clone(),
                        other => other.get_type(),
                    };
                    if let Ok(Some(t)) = self.analyze_variant_construction(
                        &enum_name,
                        id,
                        &[],
                        parent_function,
                        symbol_table,
                        diagnostics,
                    ) {
                        return Ok(t);
                    }
                }
                // Unresolved name: report and short-circuit. Statement-level callers recover
                // (poisoning the binding with `Type::Unknown`) so sibling errors still surface.
                return Err(report_with_code(
                    diagnostics,
                    e.to_string(),
                    Some(id.position),
                    "unresolved-name",
                ));
            }
        };
        // File/module-level visibility (Axis 2): a non-public top-level variable is only readable
        // from its declaring file — and ONLY when the identifier actually resolved to that global.
        // A function-local of the same name shadows it and must not trip this check (regex.dream's
        // local `g` vs a user file's top-level `g` used to collide here).
        if !(*symbol_table)
            .as_ref()
            .borrow()
            .resolves_before_global_root(&id.text)
        {
            if let Some(global) = self.globals.iter().find(|g| g.name == id.text) {
                if !self.visible_across_files(
                    &global.file_path,
                    global.visibility,
                    self.current_file.as_ref(),
                ) {
                    let decl_file = global.file_path.clone();
                    self.report_not_public(
                        "Variable",
                        &id.text,
                        &decl_file,
                        id.position,
                        diagnostics,
                    );
                }
            }
        }
        let is_local = (*symbol_table)
            .as_ref()
            .borrow()
            .resolves_before_global_root(&id.text);
        let target = if is_local {
            ide::IdeTarget::Local {
                name: id.text.clone(),
            }
        } else {
            ide::IdeTarget::Global {
                name: id.text.clone(),
            }
        };
        let summary = self.ide_summary(&r);
        self.record_ide_ref(id.position, target, summary);
        self.hir_set_var(&id.text);
        Ok(r)
    }

    /// The boxed `fun(...)` value of the function registered under `key`, invoked through
    /// synchronous `call_indirect`. An `async fun`'s constructor returns an untagged `Future` frame
    /// pointer, so boxing it as `fun(...): Future<T>` matches the WASM result and lets the caller
    /// `f(...).await` like a direct async call. Worker bodies use the same shape (`spawn_async` /
    /// `map_async` / `dispatch_async`).
    fn function_value(&mut self, id: &SyntaxToken, key: &str) -> Type {
        let Ok(sig) = self.function_table.get_function(key) else {
            self.hir_fail();
            return Type::Unknown;
        };
        let params = sig
            .parameters
            .iter()
            .map(|p| Self::type_from_name(p))
            .collect();
        let ret = if sig.is_async {
            Self::async_return_type(true, sig.return_type.clone())
        } else {
            sig.return_type.clone().unwrap_or(Type::Void)
        };
        let func_ty = Type::Function(params, Box::new(ret.clone()));
        self.hir_set_func_value(key, &func_ty, &ret);
        let summary = self.ide_summary(&func_ty);
        self.record_ide_ref(
            id.position,
            ide::IdeTarget::Callee {
                key: key.to_string(),
                label: id.text.clone(),
            },
            summary,
        );
        func_ty
    }

    /// An overloaded function taken as a value: the `fun(...)` expected type's parameter list
    /// picks the overload, since there is no argument list to resolve against.
    fn overloaded_function_value(
        &mut self,
        id: &SyntaxToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Type {
        let expected = self
            .current_expected_type
            .as_ref()
            .map(|t| Self::monomorphize_type(t, &self.current_generic_bindings));
        let message = match &expected {
            Some(Type::Function(params, _)) => {
                let names: Vec<String> = params.iter().map(Type::get_type).collect();
                if let Some(key) = self.function_table.overload_with_params(&id.text, &names) {
                    let key = key.to_string();
                    return self.function_value(id, &key);
                }
                format!(
                    "No overload of '{}' takes parameters ({})",
                    id.text,
                    names.join(", ")
                )
            }
            _ => format!(
                "'{}' is overloaded; give it a fun(...) type to select an overload",
                id.text
            ),
        };
        report_with_code(
            diagnostics,
            message,
            Some(id.position),
            "ambiguous-overload",
        );
        self.hir_fail();
        Type::Unknown
    }

    /// Reconstructs a `Type` from its canonical type-name string (as stored in function-table
    /// signatures), e.g. "int", "string", "Node", "int[]", "fun(int,int):int". Falls back to `void`
    /// if unparseable.
    pub(in crate::analyzer) fn type_from_name(name: &str) -> Type {
        // `Type::Function::get_type()` renders as `fun(<params joined by ",">):<ret>`, with no
        // spaces (see `types.rs`); reverse it here so a `fun(...)`-typed function-table parameter
        // (e.g. a `sort_by(cmp: fun(T, T): int)` parameter, or a synthesized lambda's own signature)
        // round-trips correctly instead of collapsing to a bogus struct type. Struct generic args
        // mangle to `_`-joined names (no `<`/`>`), so only `(`/`)` and `[`/`]` need nesting tracking.
        if let Some(rest) = name.strip_prefix("fun(") {
            if let Some(close) = matching_close_paren(rest) {
                let params_str = &rest[..close];
                if let Some(ret_str) = rest[close + 1..].strip_prefix(':') {
                    let params = split_top_level_commas(params_str)
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .map(|p| Self::type_from_name(&p))
                        .collect();
                    let ret = Self::type_from_name(ret_str);
                    return Type::Function(params, Box::new(ret));
                }
            }
        }
        let token = synthetic_token(TokenKind::IdentifierToken, name);
        Type::from_token(token).unwrap_or(Type::Void)
    }
}

/// Given the text immediately after a `fun(`'s opening paren, returns the byte index (into that
/// text) of the `)` that closes it, tracking `(`/`[` nesting so a nested `fun(...)` parameter or an
/// array type doesn't terminate the scan early.
fn matching_close_paren(s: &str) -> Option<usize> {
    let mut depth = 1i32;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits a `fun(...)` parameter-list string on top-level commas only, respecting `(`/`[` nesting
/// so a nested `fun(a,b):c` or array-typed parameter isn't split in the middle.
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(s[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(s[start..].to_string());
    parts
}
