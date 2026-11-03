//! Per-body facts for parameter-mode inference: which candidate parameters a body consumes
//! outright, and which it forwards to another function's parameter (decided by the fixpoint).

use dream_hir::{Binding, Callee, HExpr, HExprKind, HPattern, HPlace, HStmt, LocalId};
use dream_types::DefId;
use indexmap::{IndexMap, IndexSet};

/// How the surrounding expression uses a value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Use {
    /// Read in place: the value's owner keeps it (field read, operator operand, borrowed argument).
    Read,
    /// The value may outlive the expression: stored, returned, bound, or handed to a sink.
    Escape,
}

/// What one function body does with each of its candidate parameters.
pub(super) struct BodyFacts {
    /// Parameter index of each candidate parameter's local.
    candidates: IndexMap<LocalId, usize>,
    /// Candidates held by value: a borrowed copy does not own its fields, so writing through it
    /// or calling a method that may mutate `this` needs the sink.
    values: IndexSet<LocalId>,
    /// Candidate parameters the body consumes regardless of any callee.
    pub(super) consumed: IndexSet<usize>,
    /// `(parameter, callee def, callee parameter)`: the parameter is passed straight to a
    /// parameter whose own mode the fixpoint decides.
    pub(super) forwards: Vec<(usize, DefId, usize)>,
}

impl BodyFacts {
    pub(super) fn collect(
        candidates: IndexMap<LocalId, usize>,
        values: IndexSet<LocalId>,
        body: &[HStmt],
        inferable: &IndexSet<DefId>,
        refs: &IndexMap<DefId, Vec<bool>>,
    ) -> Self {
        let mut facts = BodyFacts {
            candidates,
            values,
            consumed: IndexSet::new(),
            forwards: Vec::new(),
        };
        let mut walker = Walker {
            facts: &mut facts,
            inferable,
            refs,
        };
        walker.stmts(body);
        facts
    }
}

struct Walker<'f> {
    facts: &'f mut BodyFacts,
    inferable: &'f IndexSet<DefId>,
    /// `ref` positions of every lowered function; a callee missing here may take any argument
    /// by `ref`.
    refs: &'f IndexMap<DefId, Vec<bool>>,
}

impl Walker<'_> {
    fn stmts(&mut self, stmts: &[HStmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &HStmt) {
        match stmt {
            HStmt::Let { value, .. } => self.expr(value, Use::Escape),
            HStmt::Assign { place, value } => {
                self.place(place);
                self.expr(value, Use::Escape);
            }
            HStmt::Expr(e) => self.expr(e, Use::Read),
            HStmt::Return(e) => {
                if let Some(e) = e {
                    self.expr(e, Use::Escape);
                }
            }
            HStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expr(cond, Use::Read);
                self.stmts(then_branch);
                self.stmts(else_branch);
            }
            HStmt::While { cond, body, .. } | HStmt::DoWhile { cond, body, .. } => {
                self.expr(cond, Use::Read);
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
                self.expr(cond, Use::Read);
                self.stmts(step);
                self.stmts(body);
            }
            HStmt::Foreach { iterable, body, .. } => {
                self.expr(iterable, Use::Read);
                self.stmts(body);
            }
            HStmt::Lock { target, body } => {
                self.expr(target, Use::Read);
                self.stmts(body);
            }
            HStmt::Cleanup { body, cleanup } => {
                self.stmts(body);
                self.expr(cleanup, Use::Read);
            }
            HStmt::Defer { budget, body } => {
                if let Some(budget) = budget {
                    self.expr(budget, Use::Read);
                }
                self.stmts(body);
            }
            // Pattern bindings alias the scrutinee's payload, so the scrutinee escapes.
            HStmt::Switch {
                scrutinee,
                arms,
                default,
            } => {
                self.expr(scrutinee, Use::Escape);
                for arm in arms {
                    if let HPattern::Const(e) = &arm.pattern {
                        self.expr(e, Use::Read);
                    }
                    self.stmts(&arm.body);
                }
                self.stmts(default);
            }
            HStmt::Await { future, .. } => self.expr(future, Use::Escape),
            HStmt::Break(_)
            | HStmt::Continue(_)
            | HStmt::DebugLine(_)
            | HStmt::SourceLine(_) => {}
        }
    }

    fn place(&mut self, place: &HPlace) {
        match place {
            HPlace::Local(local) => {
                if let Some(&index) = self.facts.candidates.get(local) {
                    self.facts.consumed.insert(index);
                }
            }
            HPlace::Global(_) => {}
            HPlace::Field { obj, .. } => {
                self.mutate(obj);
                self.expr(obj, Use::Read);
            }
            HPlace::Index { array, index } => {
                self.mutate(array);
                self.expr(array, Use::Read);
                self.expr(index, Use::Read);
            }
        }
    }

    /// Consumes the by-value candidate at the root of `target`, a place that may be written.
    fn mutate(&mut self, target: &HExpr) {
        let mut root = target;
        loop {
            match &root.kind {
                HExprKind::Field { obj: inner, .. }
                | HExprKind::UnionField { base: inner, .. }
                | HExprKind::Index { array: inner, .. } => root = inner,
                HExprKind::Var(Binding::Local(local)) => {
                    if self.facts.values.contains(local)
                        && let Some(&index) = self.facts.candidates.get(local)
                    {
                        self.facts.consumed.insert(index);
                    }
                    return;
                }
                _ => return,
            }
        }
    }

    fn reads(&mut self, exprs: &[&HExpr]) {
        for e in exprs {
            self.expr(e, Use::Read);
        }
    }

    fn escapes(&mut self, exprs: &[HExpr]) {
        for e in exprs {
            self.expr(e, Use::Escape);
        }
    }

    /// One argument of a resolved call. `take` is the call site's flag for the callee parameter
    /// at `param`; a borrowed argument is a read.
    fn arg(&mut self, arg: &HExpr, def: DefId, param: usize, take: bool) {
        if self
            .refs
            .get(&def)
            .is_none_or(|refs| refs.get(param).copied().unwrap_or(false))
        {
            self.mutate(arg);
        }
        if !take {
            self.expr(arg, Use::Read);
            return;
        }
        if let HExprKind::Var(Binding::Local(local)) = &arg.kind
            && let Some(&index) = self.facts.candidates.get(local)
            && self.inferable.contains(&def)
        {
            self.facts.forwards.push((index, def, param));
            return;
        }
        self.expr(arg, Use::Escape);
    }

    fn call_args(&mut self, callee: &Callee, args: &[HExpr], offset: usize) {
        for (i, arg) in args.iter().enumerate() {
            let param = i + offset;
            let take = callee.take_params.get(param).copied().unwrap_or(false);
            self.arg(arg, callee.def, param, take);
        }
    }

    fn expr(&mut self, e: &HExpr, ctx: Use) {
        use HExprKind as K;
        match &e.kind {
            K::Var(Binding::Local(local)) => {
                if ctx == Use::Escape
                    && let Some(&index) = self.facts.candidates.get(local)
                {
                    self.facts.consumed.insert(index);
                }
            }
            K::Var(_)
            | K::IntLit(_)
            | K::SizeOf(_)
            | K::AlignOf(_)
            | K::FloatLit(_)
            | K::BoolLit(_)
            | K::CharLit(_)
            | K::StringLit(_)
            | K::EnumValue(_)
            | K::Zero => {}
            K::Call { callee, args } => self.call_args(callee, args, 0),
            K::MethodCall {
                receiver,
                callee,
                args,
            } => {
                self.mutate(receiver);
                self.expr(receiver, Use::Read);
                self.call_args(callee, args, 1);
            }
            K::New {
                ctor,
                args,
                take_params,
                ..
            } => {
                for (i, arg) in args.iter().enumerate() {
                    let take = take_params.get(i).copied().unwrap_or(false);
                    match ctor {
                        Some(ctor) => self.arg(arg, *ctor, i + 1, take),
                        None => self.expr(arg, Use::Escape),
                    }
                }
            }
            K::InterfaceCall { receiver, args, .. } => {
                self.mutate(receiver);
                self.expr(receiver, Use::Read);
                args.iter().for_each(|arg| self.mutate(arg));
                self.escapes(args);
            }
            K::IndirectCall { target, args, .. } => {
                self.expr(target, Use::Read);
                args.iter().for_each(|arg| self.mutate(arg));
                self.escapes(args);
            }
            K::JsCall {
                target,
                via,
                method,
                args,
                ..
            } => {
                self.expr(target, Use::Escape);
                if let Some(via) = via {
                    self.expr(via, Use::Read);
                }
                if let Some(method) = method {
                    self.expr(method, Use::Read);
                }
                self.escapes(args);
            }
            K::UnionNew { args, .. } => self.escapes(args),
            K::ArrayLit { elems, .. } | K::Tuple { elems } => self.escapes(elems),
            K::Binary { lhs, rhs, .. } | K::Concat(lhs, rhs) => self.reads(&[lhs, rhs]),
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
            | K::UnionField { base: x, .. } => self.expr(x, Use::Read),
            K::Index { array, index }
            | K::CharAt(array, index, _)
            | K::ByteAt(array, index)
            | K::ArrayGetUnchecked { array, index }
            | K::ArraySlotTake { array, index, .. } => self.reads(&[array, index]),
            K::ArrayNew { len, .. } => self.expr(len, Use::Read),
            K::RawStore { address, value } => {
                self.expr(address, Use::Read);
                self.expr(value, Use::Escape);
            }
            K::ArrayRealloc { array, new_len, .. } => {
                self.expr(array, Use::Escape);
                self.expr(new_len, Use::Read);
            }
            K::ArrayElemsCopy {
                dst,
                dst_off,
                src,
                src_off,
                count,
                ..
            } => self.reads(&[dst, dst_off, src, src_off, count]),
            K::ArrayElemsFill {
                dst,
                dst_off,
                count,
                ..
            } => self.reads(&[dst, dst_off, count]),
            K::ArraySlotSwap {
                array,
                left,
                other_array,
                right,
                ..
            } => self.reads(&[array, left, other_array, right]),
            K::ArraySetUnchecked {
                array,
                index,
                value,
            } => {
                self.reads(&[array, index]);
                self.expr(value, Use::Escape);
            }
            K::ForceFree(x) | K::Await(x) => self.expr(x, Use::Escape),
            // The result is the operand itself, so it inherits the surrounding use.
            K::Cast(x) => self.expr(x, ctx),
            K::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                self.expr(cond, Use::Read);
                self.expr(then_expr, ctx);
                self.expr(else_expr, ctx);
            }
        }
    }
}
