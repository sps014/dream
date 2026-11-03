//! Parameter ownership modes are inferred from function bodies, so users never write `sink`.
//! A parameter is a sink when the body stores, returns, binds, reassigns or captures it, or
//! passes it to another sink; every other unmarked reference parameter borrows. A by-value
//! aggregate holding references borrows the same way unless the body writes into it. Recursive and
//! mutually recursive functions converge in one fixpoint that only moves parameters from borrow
//! to sink. Modes are per declaration (joined across generic instances), and every call site's
//! take flags are rewritten from the result, so callers and callees always agree.
//!
//! A function whose calling convention is fixed by something this pass cannot see keeps its
//! declared modes: async functions (their frame outlives the call), exported functions, interface
//! implementations and functions used as values (indirect callers), and slot getters. Parameters
//! whose destruction may run a `del` stay sinks, so inference never moves a destructor.

mod aggregates;
mod destructors;
mod facts;

use dream_hir::{
    HExpr, HExprKind, HFunction, HGlobal, HPlace, HStmt, InterfaceTable, LayoutTable, ParamMode,
};
use dream_types::{DefId, TypeInterner};
use destructors::Destructors;
use facts::BodyFacts;
use indexmap::{IndexMap, IndexSet};

/// Program facts the inference reads but never changes.
pub(super) struct Program<'p> {
    pub(super) interfaces: &'p InterfaceTable,
    pub(super) exports: &'p IndexSet<DefId>,
    /// `(function def, parameter index)` of every explicit `sink` parameter.
    pub(super) pinned_sink: &'p IndexSet<(DefId, usize)>,
    pub(super) interner: &'p TypeInterner,
    pub(super) layouts: &'p LayoutTable,
}

/// Infers parameter modes for `functions` and rewrites every call site's take flags to match.
pub(super) fn infer_param_modes(
    functions: &mut [HFunction],
    globals: &mut [HGlobal],
    program: &Program<'_>,
) {
    let inferable = inferable_defs(functions, globals, program.interfaces, program.exports);
    let mut destructors = Destructors::new(program.interner, program.layouts);

    // `true` = sink. Starts from the declared mode; candidates start as borrow.
    let mut modes: IndexMap<DefId, Vec<bool>> = IndexMap::new();
    let mut candidates: IndexSet<(DefId, usize)> = IndexSet::new();
    for f in functions.iter() {
        if !inferable.contains(&f.def) {
            continue;
        }
        let declared = f
            .params
            .iter()
            .map(|p| matches!(p.mode, ParamMode::Sink | ParamMode::Share));
        let entry = modes.entry(f.def).or_insert_with(|| declared.collect());
        for (i, p) in f.params.iter().enumerate() {
            if p.mode == ParamMode::Sink
                && (program.interner.is_rc_tracked(p.ty)
                    || aggregates::owns_references(program.interner, program.layouts, p.ty))
                && !program.pinned_sink.contains(&(f.def, i))
                && !destructors.may_run(p.ty)
            {
                candidates.insert((f.def, i));
                entry[i] = false;
            }
        }
    }

    let refs: IndexMap<DefId, Vec<bool>> = functions
        .iter()
        .map(|f| (f.def, f.params.iter().map(|p| p.mode == ParamMode::Ref).collect()))
        .collect();
    let mut forwards: Vec<((DefId, usize), (DefId, usize))> = Vec::new();
    for f in functions.iter() {
        if !inferable.contains(&f.def) {
            continue;
        }
        let locals = f
            .params
            .iter()
            .enumerate()
            .filter(|(i, _)| candidates.contains(&(f.def, *i)))
            .map(|(i, p)| (p.local, i))
            .collect();
        let values = f
            .params
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                candidates.contains(&(f.def, *i)) && !program.interner.is_rc_tracked(p.ty)
            })
            .map(|(_, p)| p.local)
            .collect();
        let facts = BodyFacts::collect(locals, values, &f.body, &inferable, &refs);
        for index in facts.consumed {
            modes[&f.def][index] = true;
        }
        for (index, def, param) in facts.forwards {
            forwards.push(((f.def, index), (def, param)));
        }
    }

    loop {
        let mut changed = false;
        for &((def, index), (callee, param)) in &forwards {
            if modes[&def][index] {
                continue;
            }
            let callee_sinks = modes
                .get(&callee)
                .and_then(|m| m.get(param))
                .copied()
                .unwrap_or(true);
            if callee_sinks {
                modes[&def][index] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    for f in functions.iter_mut() {
        if let Some(sinks) = modes.get(&f.def) {
            for (i, p) in f.params.iter_mut().enumerate() {
                if candidates.contains(&(f.def, i)) && !sinks[i] {
                    p.mode = ParamMode::Borrow;
                }
            }
        }
    }
    let mut rewrite = CallRewrite { modes: &modes };
    for f in functions.iter_mut() {
        rewrite.stmts(&mut f.body);
    }
    for g in globals.iter_mut() {
        if let Some(init) = &mut g.init {
            rewrite.expr(init);
        }
    }
}

/// Functions whose modes this pass may change: every caller is a direct call it can rewrite.
fn inferable_defs(
    functions: &[HFunction],
    globals: &[HGlobal],
    interfaces: &InterfaceTable,
    exports: &IndexSet<DefId>,
) -> IndexSet<DefId> {
    let mut fixed: IndexSet<DefId> = exports.clone();
    fixed.extend(
        interfaces
            .impls
            .iter()
            .flat_map(|i| &i.entries)
            .flat_map(|(_, slots)| slots.iter().flatten().copied()),
    );
    let mut edges = dream_hir::HirEdges::default();
    for f in functions {
        dream_hir::hir_body_edges(&f.body, &mut edges);
    }
    for g in globals {
        if let Some(init) = &g.init {
            dream_hir::hir_expr_edges(init, &mut edges);
        }
    }
    fixed.extend(edges.funcrefs.into_iter().map(|(def, _)| def));
    let mut async_or_fixed: IndexSet<DefId> = IndexSet::new();
    for f in functions {
        if f.is_async || f.borrowed_result {
            async_or_fixed.insert(f.def);
        }
    }
    functions
        .iter()
        .map(|f| f.def)
        .filter(|def| !fixed.contains(def) && !async_or_fixed.contains(def))
        .collect()
}

/// Mutable walk over every expression in a body; `visit` sees each expression before its
/// children.
trait ExprVisitor {
    fn visit(&mut self, e: &mut HExpr);

    fn stmts(&mut self, stmts: &mut [HStmt]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, stmt: &mut HStmt) {
        match stmt {
            HStmt::Let { value, .. } | HStmt::Expr(value) | HStmt::Await { future: value, .. } => {
                self.expr(value)
            }
            HStmt::Assign { place, value } => {
                match place {
                    HPlace::Field { obj, .. } => self.expr(obj),
                    HPlace::Index { array, index } => {
                        self.expr(array);
                        self.expr(index);
                    }
                    HPlace::Local(_) | HPlace::Global(_) => {}
                }
                self.expr(value);
            }
            HStmt::Return(e) => {
                if let Some(e) = e {
                    self.expr(e);
                }
            }
            HStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expr(cond);
                self.stmts(then_branch);
                self.stmts(else_branch);
            }
            HStmt::While { cond, body, .. } | HStmt::DoWhile { cond, body, .. } => {
                self.expr(cond);
                self.stmts(body);
            }
            HStmt::For {
                init,
                cond,
                step,
                body,
                ..
            } => {
                self.stmts(init);
                self.expr(cond);
                self.stmts(step);
                self.stmts(body);
            }
            HStmt::Foreach { iterable, body, .. } => {
                self.expr(iterable);
                self.stmts(body);
            }
            HStmt::Lock { target, body } => {
                self.expr(target);
                self.stmts(body);
            }
            HStmt::Cleanup { body, cleanup } => {
                self.stmts(body);
                self.expr(cleanup);
            }
            HStmt::Defer { budget, body } => {
                if let Some(budget) = budget {
                    self.expr(budget);
                }
                self.stmts(body);
            }
            HStmt::Switch {
                scrutinee,
                arms,
                default,
            } => {
                self.expr(scrutinee);
                for arm in arms {
                    if let dream_hir::HPattern::Const(e) = &mut arm.pattern {
                        self.expr(e);
                    }
                    self.stmts(&mut arm.body);
                }
                self.stmts(default);
            }
            HStmt::Break(_)
            | HStmt::Continue(_)
            | HStmt::DebugLine(_)
            | HStmt::SourceLine(_) => {}
        }
    }

    fn expr(&mut self, e: &mut HExpr) {
        self.visit(e);
        for child in children(&mut e.kind) {
            self.expr(child);
        }
    }
}

/// Every direct sub-expression of `kind`.
fn children(kind: &mut HExprKind) -> Vec<&mut HExpr> {
    use HExprKind as K;
    match kind {
        K::Var(_)
        | K::IntLit(_)
        | K::SizeOf(_)
        | K::AlignOf(_)
        | K::FloatLit(_)
        | K::BoolLit(_)
        | K::CharLit(_)
        | K::StringLit(_)
        | K::EnumValue(_)
        | K::Zero => Vec::new(),
        K::Call { args, .. }
        | K::New { args, .. }
        | K::UnionNew { args, .. }
        | K::ArrayLit { elems: args, .. }
        | K::Tuple { elems: args } => args.iter_mut().collect(),
        K::MethodCall { receiver, args, .. }
        | K::InterfaceCall { receiver, args, .. }
        | K::IndirectCall {
            target: receiver,
            args,
            ..
        } => std::iter::once(&mut **receiver).chain(args.iter_mut()).collect(),
        K::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => std::iter::once(&mut **target)
            .chain(via.as_deref_mut())
            .chain(method.as_deref_mut())
            .chain(args.iter_mut())
            .collect(),
        K::Binary { lhs, rhs, .. }
        | K::Concat(lhs, rhs)
        | K::Index {
            array: lhs,
            index: rhs,
        }
        | K::CharAt(lhs, rhs, _)
        | K::ByteAt(lhs, rhs)
        | K::ArrayGetUnchecked {
            array: lhs,
            index: rhs,
        }
        | K::ArraySlotTake {
            array: lhs,
            index: rhs,
            ..
        }
        | K::RawStore {
            address: lhs,
            value: rhs,
        }
        | K::ArrayRealloc {
            array: lhs,
            new_len: rhs,
            ..
        } => vec![&mut **lhs, &mut **rhs],
        K::Unary { operand: x, .. }
        | K::ArrayLen(x)
        | K::StrLen(x)
        | K::StrByteSize(x)
        | K::HashCode(x)
        | K::ToString(x)
        | K::Discriminant(x)
        | K::RawLoad(x)
        | K::ToBytes(x)
        | K::FromBytes(x)
        | K::WireEncode(x)
        | K::WireDecode(x)
        | K::EnumName { value: x, .. }
        | K::IsType { value: x, .. }
        | K::TypeName { value: x }
        | K::Print { arg: x, .. }
        | K::Field { obj: x, .. }
        | K::UnionField { base: x, .. }
        | K::ArrayNew { len: x, .. }
        | K::ForceFree(x)
        | K::Await(x)
        | K::Cast(x) => vec![&mut **x],
        K::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => vec![
            &mut **dst,
            &mut **dst_off,
            &mut **src,
            &mut **src_off,
            &mut **count,
        ],
        K::ArrayElemsFill {
            dst,
            dst_off,
            count,
            ..
        } => vec![&mut **dst, &mut **dst_off, &mut **count],
        K::ArraySlotSwap {
            array,
            left,
            other_array,
            right,
            ..
        } => vec![
            &mut **array,
            &mut **left,
            &mut **other_array,
            &mut **right,
        ],
        K::ArraySetUnchecked {
            array,
            index,
            value,
        } => vec![&mut **array, &mut **index, &mut **value],
        K::Ternary {
            cond,
            then_expr,
            else_expr,
        } => vec![&mut **cond, &mut **then_expr, &mut **else_expr],
    }
}

/// Rewrites call-site take flags from the inferred modes.
struct CallRewrite<'m> {
    modes: &'m IndexMap<DefId, Vec<bool>>,
}

impl ExprVisitor for CallRewrite<'_> {
    fn visit(&mut self, e: &mut HExpr) {
        match &mut e.kind {
            HExprKind::Call { callee, .. } | HExprKind::MethodCall { callee, .. } => {
                if let Some(sinks) = self.modes.get(&callee.def)
                    && callee.take_params.len() == sinks.len()
                {
                    callee.take_params.clone_from(sinks);
                }
            }
            HExprKind::New {
                ctor: Some(ctor),
                take_params,
                ..
            } => {
                if let Some(sinks) = self.modes.get(ctor)
                    && take_params.len() + 1 == sinks.len()
                {
                    take_params.clone_from_slice(&sinks[1..]);
                }
            }
            _ => {}
        }
    }
}
