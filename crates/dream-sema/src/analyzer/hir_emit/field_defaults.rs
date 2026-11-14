//! Default field values. A new object starts zeroed: numbers are 0, strings are empty and
//! `Option` fields are `None`. A field whose zero is not a value of its type (an array, a class,
//! an interface or function value, a union, or a value type holding one) starts as an empty
//! array, as `T()`, or as the union's first variant without fields when the constructor does not
//! assign it on every path. Explicit constructors
//! assign those defaults in a prologue; the implicit constructor carries them on the `New` node.
//! A field with no such default must be assigned by every constructor, which is a compile error
//! otherwise.

use super::*;
use crate::symbol_table::SymbolTable;
use dream_syntax::nodes::types::CONSTRUCTOR_NAME;
use dream_syntax::nodes::{ExpressionNode, StatementNode};
use dream_text::text_span::TextSpan;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

/// Value-type nesting followed before a zero value is assumed valid.
const MAX_DEFAULT_DEPTH: usize = 16;

/// The defaults for a constructor, and the unassigned fields that have none.
struct FieldDefaults {
    values: Vec<(usize, HExpr)>,
    missing: Vec<(String, TypeId)>,
}

impl<'a> Analyzer<'a> {
    /// Assigns defaults at the start of a constructor to the fields its body does not assign on
    /// every path, and reports the unassigned fields that have no default.
    pub(in crate::analyzer) fn hir_constructor_field_defaults(
        &mut self,
        function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) {
        if function.is_extern
            || function.is_static
            || function
                .file_path
                .as_deref()
                .is_some_and(dream_stdlib::is_std_source)
        {
            return;
        }
        let Some(this) = function
            .parameters
            .iter()
            .find(|p| p.name.text == THIS_NAME)
        else {
            return;
        };
        let owner = self.type_ctx.lower(&Self::monomorphize_type(
            &this.type_,
            &self.current_generic_bindings,
        ));
        let is_constructor = self.function_declaration(function).is_some_and(|identity| {
            self.function_table
                .method_candidates(owner, CONSTRUCTOR_NAME)
                .contains(&identity)
        });
        if !is_constructor {
            return;
        }
        let assigned = definitely_assigned(function.body);
        let saved = self.hir.last.take();
        let defaults = self.field_defaults(owner, &assigned, function, symbol_table, diagnostics);
        let owner_name = self.type_id_display(owner);
        for (field, ty) in defaults.missing {
            let message = format!(
                "constructor of '{owner_name}' must assign field '{field}' on every path: '{}' has no default value",
                self.type_id_display(ty)
            );
            report_once(diagnostics, message, function.name.position);
        }
        for (field, value) in defaults.values {
            let _ = self.analyze_expression(
                &ExpressionNode::Identifier(this.name.clone()),
                function,
                symbol_table,
                &mut DiagnosticBag::new(diagnostics.file_path.clone()),
            );
            let Some(receiver) = self.hir.last.take() else {
                continue;
            };
            self.push_stmt(HStmt::Assign {
                place: HPlace::Field {
                    obj: Box::new(receiver),
                    field,
                },
                value,
            });
        }
        self.hir.last = saved;
    }

    /// Defaults the implicit constructor of `owner` stores after allocation. A field with no
    /// default is reported at `call`, the implicit constructor call.
    pub(in crate::analyzer) fn implicit_constructor_field_inits(
        &mut self,
        owner: TypeId,
        call: TextSpan,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Vec<(usize, HExpr)> {
        let library = self
            .struct_info(owner)
            .and_then(|info| info.file_path.clone())
            .is_some_and(|path| dream_stdlib::is_std_source(&path));
        if library {
            return Vec::new();
        }
        let saved = self.hir.last.take();
        let defaults = self.field_defaults(
            owner,
            &BTreeSet::new(),
            parent_function,
            symbol_table,
            diagnostics,
        );
        self.hir.last = saved;
        let owner_name = self.type_id_display(owner);
        for (field, ty) in defaults.missing {
            let message = format!(
                "'{owner_name}' needs a constructor that assigns field '{field}': '{}' has no default value",
                self.type_id_display(ty)
            );
            report_once(diagnostics, message, call);
        }
        defaults.values
    }

    fn field_defaults(
        &mut self,
        owner: TypeId,
        assigned: &BTreeSet<String>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &DiagnosticBag,
    ) -> FieldDefaults {
        let mut defaults = FieldDefaults {
            values: Vec::new(),
            missing: Vec::new(),
        };
        let Some(info) = self.struct_info(owner) else {
            return defaults;
        };
        let fields: Vec<(usize, String, TypeId)> = info
            .fields
            .iter()
            .enumerate()
            .filter(|(_, (name, field))| {
                !field.is_weak && !field.is_unowned && !assigned.contains(*name)
            })
            .map(|(index, (name, field))| (index, name.clone(), field.ty))
            .collect();
        for (index, name, ty) in fields {
            if self.zero_is_value(ty, 0) {
                continue;
            }
            match self.default_value(ty, owner, parent_function, symbol_table, diagnostics) {
                Some(value) => defaults.values.push((index, value)),
                None => defaults.missing.push((name, ty)),
            }
        }
        defaults
    }

    fn default_value(
        &mut self,
        ty: TypeId,
        owner: TypeId,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &DiagnosticBag,
    ) -> Option<HExpr> {
        match self.type_ctx.interner.kind(ty) {
            TyKind::Array(elem) => {
                let int = self.type_ctx.interner.int();
                Some(HExpr::new(
                    ty,
                    HExprKind::ArrayNew {
                        elem_ty: *elem,
                        len: Box::new(HExpr::new(int, HExprKind::IntLit(0))),
                        closure_env: false,
                    },
                ))
            }
            TyKind::Union(def, _) => {
                let def = *def;
                let variant = self.empty_variant(ty)? as usize;
                Some(HExpr::new(
                    ty,
                    HExprKind::UnionNew {
                        def,
                        variant,
                        args: Vec::new(),
                    },
                ))
            }
            TyKind::Struct(..) => {
                if self.default_builds(ty, owner, &mut BTreeSet::new()) {
                    return None;
                }
                let Type::Struct(name, generic_args) = self.type_ctx.syntax_type(ty) else {
                    return None;
                };
                let call = ExpressionNode::FunctionCall(name, generic_args, Vec::new());
                let ok = self.hir.ok;
                let mut scratch = DiagnosticBag::new(diagnostics.file_path.clone());
                let built =
                    self.analyze_expression(&call, parent_function, symbol_table, &mut scratch);
                let value = self.hir.last.take();
                self.hir.ok = ok;
                match built {
                    Ok(_) if !scratch.has_errors() => value,
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Whether a zeroed `ty` is a value of its type.
    fn zero_is_value(&mut self, ty: TypeId, depth: usize) -> bool {
        if depth > MAX_DEFAULT_DEPTH {
            return true;
        }
        let fields: Vec<TypeId> = match self.type_ctx.interner.kind(ty).clone() {
            TyKind::Array(_) | TyKind::Interface(..) | TyKind::Func(..) | TyKind::Object => {
                return false;
            }
            TyKind::Struct(..) if self.type_ctx.interner.is_value_type(ty) => {
                let Some(info) = self.struct_info(ty) else {
                    return false;
                };
                info.fields.values().map(|field| field.ty).collect()
            }
            TyKind::Struct(..) => return false,
            TyKind::Tuple(elems) => elems,
            TyKind::Union(..) => {
                self.instantiate_union(ty);
                let interner = &self.type_ctx.interner;
                // A niche union reads null as its empty variant, and a zeroed value union is its
                // variant with discriminant 0.
                return interner.is_niche_union(ty)
                    || (interner.is_value_type(ty) && self.empty_variant(ty) == Some(0));
            }
            _ => return true,
        };
        fields
            .into_iter()
            .all(|field| self.zero_is_value(field, depth + 1))
    }

    /// The discriminant of the first variant of union `ty` that carries no fields.
    fn empty_variant(&self, ty: TypeId) -> Option<i32> {
        self.union_info(ty)?
            .variants
            .iter()
            .find(|variant| variant.fields.is_empty())
            .map(|variant| variant.discriminant)
    }

    /// Instantiates the generic union `ty` so its layout is known.
    fn instantiate_union(&mut self, ty: TypeId) {
        if self.union_info(ty).is_some() {
            return;
        }
        let Type::Struct(name, Some(args)) = self.type_ctx.syntax_type(ty) else {
            return;
        };
        let mut scratch = DiagnosticBag::new(None);
        self.ensure_union_instantiated(&name.text, &args, &name.position, &mut scratch);
    }

    /// Whether building `ty`'s default constructs `owner` again, directly or through the
    /// defaults of its own fields.
    fn default_builds(
        &mut self,
        ty: TypeId,
        owner: TypeId,
        visited: &mut BTreeSet<TypeId>,
    ) -> bool {
        if ty == owner {
            return true;
        }
        if !visited.insert(ty) {
            return false;
        }
        let Some(info) = self.struct_info(ty) else {
            return false;
        };
        let fields: Vec<TypeId> = info
            .fields
            .values()
            .filter(|field| !field.is_weak && !field.is_unowned)
            .map(|field| field.ty)
            .collect();
        fields.into_iter().any(|field| {
            matches!(self.type_ctx.interner.kind(field), TyKind::Struct(..))
                && !self.zero_is_value(field, 0)
                && self.default_builds(field, owner, visited)
        })
    }
}

/// Reports `message` unless an earlier instantiation of the same declaration already did.
fn report_once(diagnostics: &mut DiagnosticBag, message: String, span: TextSpan) {
    let reported = diagnostics
        .errors()
        .any(|error| error.span == Some(span) && error.message == message);
    if !reported {
        diagnostics.report_error(message, Some(span));
    }
}

/// Fields every path through `body` assigns through `this.<field> = …` before it returns or ends.
fn definitely_assigned(body: &[StatementNode<'_>]) -> BTreeSet<String> {
    let mut returns = Vec::new();
    let end = block(body, BTreeSet::new(), &mut returns);
    let mut paths = returns.into_iter().chain(end);
    let Some(first) = paths.next() else {
        return BTreeSet::new();
    };
    paths.fold(first, |all, path| {
        all.intersection(&path).cloned().collect()
    })
}

/// The fields assigned when control falls off the end of `statements`, or `None` when it never
/// does. Each `return` records the fields assigned on its path in `returns`.
fn block(
    statements: &[StatementNode<'_>],
    mut assigned: BTreeSet<String>,
    returns: &mut Vec<BTreeSet<String>>,
) -> Option<BTreeSet<String>> {
    for statement in statements {
        assigned = step(statement, assigned, returns)?;
    }
    Some(assigned)
}

fn step(
    statement: &StatementNode<'_>,
    mut assigned: BTreeSet<String>,
    returns: &mut Vec<BTreeSet<String>>,
) -> Option<BTreeSet<String>> {
    match statement {
        StatementNode::MemberAssignment(target, field, _) => {
            if matches!(target, ExpressionNode::Identifier(id) if id.text == THIS_NAME) {
                assigned.insert(field.text.clone());
            }
            Some(assigned)
        }
        StatementNode::Return(_) => {
            returns.push(assigned);
            None
        }
        StatementNode::IfElse(_, then, else_ifs, otherwise) => {
            let Some(otherwise) = otherwise else {
                skip(then, &assigned, returns);
                for (_, body) in else_ifs {
                    skip(body, &assigned, returns);
                }
                return Some(assigned);
            };
            let branches = std::iter::once(*then)
                .chain(else_ifs.iter().map(|(_, body)| *body))
                .chain(std::iter::once(*otherwise));
            join(branches, &assigned, returns)
        }
        StatementNode::Switch(_, cases, default) => {
            let Some(default) = default else {
                for (_, body) in cases {
                    skip(body, &assigned, returns);
                }
                return Some(assigned);
            };
            let branches = cases
                .iter()
                .map(|(_, body)| *body)
                .chain(std::iter::once(*default));
            join(branches, &assigned, returns)
        }
        // A loop body may run zero times, so only its returns are recorded.
        StatementNode::While(_, body)
        | StatementNode::DoWhile(body, _)
        | StatementNode::For(_, _, _, body)
        | StatementNode::ForEach(_, _, _, _, body) => {
            skip(body, &assigned, returns);
            Some(assigned)
        }
        StatementNode::AwaitForEach(_, inner) | StatementNode::Labeled(_, inner) => {
            skip(std::slice::from_ref(*inner), &assigned, returns);
            Some(assigned)
        }
        StatementNode::Lock(_, body) | StatementNode::Overflow(_, _, body) => {
            block(body, assigned, returns)
        }
        _ => Some(assigned),
    }
}

/// Records the returns inside a body whose assignments may not happen.
fn skip(
    body: &[StatementNode<'_>],
    assigned: &BTreeSet<String>,
    returns: &mut Vec<BTreeSet<String>>,
) {
    let _ = block(body, assigned.clone(), returns);
}

/// The fields every falling-through branch assigns, or `None` when no branch falls through.
fn join<'s>(
    branches: impl Iterator<Item = &'s [StatementNode<'s>]>,
    assigned: &BTreeSet<String>,
    returns: &mut Vec<BTreeSet<String>>,
) -> Option<BTreeSet<String>> {
    let mut joined: Option<BTreeSet<String>> = None;
    for branch in branches {
        if let Some(after) = block(branch, assigned.clone(), returns) {
            joined = Some(match joined {
                Some(previous) => previous.intersection(&after).cloned().collect(),
                None => after,
            });
        }
    }
    joined
}
