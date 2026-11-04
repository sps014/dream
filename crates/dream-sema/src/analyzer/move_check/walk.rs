//! Walks one function body in evaluation order, moving tracked locals and recording every use
//! of a local that is moved or maybe moved on some path reaching it.

use super::Context;
use super::state::{Destination, Move, State, Status};
use dream_hir::{Binding, Callee, HExpr, HExprKind, HPattern, HPlace, HSpan, HStmt, LocalId};
use dream_types::DefId;
use indexmap::IndexSet;

/// Loops re-run their body until the moved set stops changing; each pass can only add moves.
const MAX_LOOP_PASSES: usize = 64;

/// A use of a moved local.
pub(super) struct Finding {
    pub(super) local: LocalId,
    pub(super) span: HSpan,
    pub(super) status: Status,
}

struct LoopFrame {
    label: Option<String>,
    exits: State,
    continues: State,
}

#[derive(Clone)]
enum Mode {
    Read,
    Move(Destination),
}

pub(super) struct Walker<'c> {
    cx: &'c Context<'c>,
    tracked: &'c IndexSet<LocalId>,
    state: State,
    loops: Vec<LoopFrame>,
    reporting: bool,
    enclosing: HSpan,
    pub(super) findings: Vec<Finding>,
}

impl<'c> Walker<'c> {
    pub(super) fn new(cx: &'c Context<'c>, tracked: &'c IndexSet<LocalId>) -> Self {
        Walker {
            cx,
            tracked,
            state: State::entry(),
            loops: Vec::new(),
            reporting: true,
            enclosing: HSpan::NONE,
            findings: Vec::new(),
        }
    }

    pub(super) fn stmts(&mut self, stmts: &[HStmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &HStmt) {
        match stmt {
            HStmt::Let { local, value, .. } => {
                let name = self.cx.local_name(*local);
                let mode = if super::is_synthetic(&name) {
                    Mode::Read
                } else {
                    Mode::Move(Destination::Local(name))
                };
                self.expr(value, mode);
                self.state.moved.shift_remove(local);
            }
            HStmt::Assign { place, value } => {
                let destination = match place {
                    HPlace::Local(local) => Destination::Local(self.cx.local_name(*local)),
                    HPlace::Global(_) => Destination::Place(None),
                    HPlace::Field { obj, field } => Destination::Place(
                        self.cx
                            .place_text(obj)
                            .zip(self.cx.field_name(obj.ty, *field))
                            .map(|(obj, field)| format!("{obj}.{field}")),
                    ),
                    HPlace::Index { array, index } => Destination::Place(
                        self.cx
                            .place_text(array)
                            .zip(self.cx.place_text(index))
                            .map(|(array, index)| format!("{array}[{index}]")),
                    ),
                };
                match place {
                    HPlace::Local(_) | HPlace::Global(_) => {}
                    HPlace::Field { obj, .. } => self.expr(obj, Mode::Read),
                    HPlace::Index { array, index } => {
                        self.expr(array, Mode::Read);
                        self.expr(index, Mode::Read);
                    }
                }
                // A closure shares the variables it captures with the enclosing scope.
                let weak = match place {
                    HPlace::Field { obj, field } => self.cx.is_weak_field(obj.ty, *field),
                    HPlace::Index { array, .. } => matches!(&array.kind,
                        HExprKind::Var(dream_hir::Binding::Local(local))
                            if self.cx.local_name(*local)
                                == dream_abi::intrinsics::CLOSURE_ENV_ARRAY_LOCAL),
                    _ => false,
                };
                match &destination {
                    _ if weak => self.observe(value),
                    Destination::Local(name) if super::is_synthetic(name) => {
                        self.expr(value, Mode::Read)
                    }
                    _ => self.expr(value, Mode::Move(destination)),
                }
                if let HPlace::Local(local) = place {
                    self.state.moved.shift_remove(local);
                }
            }
            HStmt::Expr(e) => self.expr(e, Mode::Read),
            HStmt::Return(value) => {
                if let Some(value) = value {
                    self.expr(value, Mode::Read);
                }
                self.state = State::unreachable();
            }
            HStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.expr(cond, Mode::Read);
                let start = self.state.clone();
                self.stmts(then_branch);
                let then_end = std::mem::replace(&mut self.state, start);
                self.stmts(else_branch);
                self.state = then_end.join(&self.state);
            }
            HStmt::While { cond, body, label } => self.looping(label, |w| {
                w.expr(cond, Mode::Read);
                w.exit_here();
                w.stmts(body);
            }),
            HStmt::DoWhile { cond, body, label } => self.looping(label, |w| {
                w.stmts(body);
                w.continue_here();
                w.expr(cond, Mode::Read);
                w.exit_here();
            }),
            HStmt::For {
                init,
                cond,
                step,
                body,
                label,
            } => {
                self.stmts(init);
                self.looping(label, |w| {
                    w.expr(cond, Mode::Read);
                    w.exit_here();
                    w.stmts(body);
                    w.continue_here();
                    w.stmts(step);
                });
            }
            HStmt::Foreach {
                elem,
                iterable,
                body,
                label,
            } => {
                self.expr(iterable, Mode::Read);
                self.looping(label, |w| {
                    w.exit_here();
                    w.state.moved.shift_remove(elem);
                    w.stmts(body);
                });
            }
            HStmt::Lock { target, body } => {
                self.expr(target, Mode::Read);
                self.stmts(body);
            }
            HStmt::Cleanup { body, cleanup } => {
                self.stmts(body);
                self.expr(cleanup, Mode::Read);
            }
            HStmt::Defer { budget, body } => {
                if let Some(budget) = budget {
                    self.expr(budget, Mode::Read);
                }
                self.stmts(body);
            }
            HStmt::Switch {
                scrutinee,
                arms,
                default,
            } => {
                self.expr(scrutinee, Mode::Read);
                let start = self.state.clone();
                let mut end = State::unreachable();
                for arm in arms {
                    self.state = start.clone();
                    if let HPattern::Const(e) = &arm.pattern {
                        self.expr(e, Mode::Read);
                    }
                    if let HPattern::Variant { bindings, .. } = &arm.pattern {
                        for binding in bindings {
                            self.state.moved.shift_remove(binding);
                        }
                    }
                    self.stmts(&arm.body);
                    end = end.join(&self.state);
                }
                self.state = start;
                self.stmts(default);
                self.state = end.join(&self.state);
            }
            HStmt::Break(label) => {
                let state = std::mem::replace(&mut self.state, State::unreachable());
                if let Some(frame) = self.frame(label) {
                    frame.exits = frame.exits.join(&state);
                }
            }
            HStmt::Continue(label) => {
                let state = std::mem::replace(&mut self.state, State::unreachable());
                if let Some(frame) = self.frame(label) {
                    frame.continues = frame.continues.join(&state);
                }
            }
            HStmt::Await { future, .. } => self.expr(future, Mode::Read),
            HStmt::DebugLine(_) | HStmt::SourceLine(_) => {}
        }
    }

    fn frame(&mut self, label: &Option<String>) -> Option<&mut LoopFrame> {
        match label {
            None => self.loops.last_mut(),
            Some(label) => self
                .loops
                .iter_mut()
                .rev()
                .find(|frame| frame.label.as_ref() == Some(label)),
        }
    }

    fn exit_here(&mut self) {
        let state = self.state.clone();
        if let Some(frame) = self.loops.last_mut() {
            frame.exits = frame.exits.join(&state);
        }
    }

    fn continue_here(&mut self) {
        if let Some(frame) = self.loops.last_mut() {
            let continues = std::mem::replace(&mut frame.continues, State::unreachable());
            self.state = self.state.join(&continues);
        }
    }

    /// Runs `pass` (one trip around the loop, from the head back to the head) until the head
    /// state is stable, then once more with reporting on. Breaks and the condition's false edge
    /// join into the state after the loop.
    fn looping(&mut self, label: &Option<String>, mut pass: impl FnMut(&mut Self)) {
        let entry = self.state.clone();
        let reporting = self.reporting;
        self.reporting = false;
        let mut head = entry.clone();
        for _ in 0..MAX_LOOP_PASSES {
            self.state = head.clone();
            self.run_pass(label, &mut pass);
            let next = entry.join(&self.state);
            if next == head {
                break;
            }
            head = next;
        }
        self.reporting = reporting;
        self.state = head;
        let frame = self.run_pass(label, &mut pass);
        self.state = frame.exits;
    }

    fn run_pass(&mut self, label: &Option<String>, pass: &mut impl FnMut(&mut Self)) -> LoopFrame {
        let fresh = || LoopFrame {
            label: label.clone(),
            exits: State::unreachable(),
            continues: State::unreachable(),
        };
        self.loops.push(fresh());
        pass(self);
        let mut frame = self.loops.pop().unwrap_or_else(fresh);
        self.state = self.state.join(&frame.continues);
        frame.continues = State::unreachable();
        frame
    }

    fn use_local(&mut self, local: LocalId, span: HSpan) {
        if !self.reporting || !self.state.reachable {
            return;
        }
        if let Some(status) = self.state.moved.get(&local) {
            self.findings.push(Finding {
                local,
                span,
                status: status.clone(),
            });
        }
    }

    fn expr(&mut self, e: &HExpr, mode: Mode) {
        // Compiler-built nodes (closure captures) carry no span; their uses point at the nearest
        // enclosing source expression, which is the construct that introduced them.
        let outer = self.enclosing;
        if !e.span.is_none() {
            self.enclosing = e.span;
        }
        self.expr_kind(e, mode);
        self.enclosing = outer;
    }

    fn expr_kind(&mut self, e: &HExpr, mode: Mode) {
        use HExprKind as K;
        match &e.kind {
            K::Var(Binding::Local(local)) => {
                self.use_local(*local, self.enclosing);
                if let Mode::Move(to) = mode
                    && self.tracked.contains(local)
                    && self.state.reachable
                {
                    self.state
                        .moved
                        .insert(*local, Status::Moved(Move { span: self.enclosing, to }));
                }
            }
            K::Cast(inner) => self.expr(inner, mode),
            K::Ternary {
                cond,
                then_expr,
                else_expr,
            } => {
                self.expr(cond, Mode::Read);
                let start = self.state.clone();
                self.expr(then_expr, mode.clone());
                let then_end = std::mem::replace(&mut self.state, start);
                self.expr(else_expr, mode);
                self.state = then_end.join(&self.state);
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
            K::Call { callee, args } => self.call(callee, args, 0),
            K::MethodCall {
                receiver,
                callee,
                args,
            } => {
                self.expr(receiver, Mode::Read);
                match args.as_slice() {
                    [key, value] if self.cx.setters.contains(&callee.def) => {
                        self.expr(key, Mode::Read);
                        let mode = if self.cx.kept.contains(&(callee.def, 2)) {
                            let place = self.cx.place_text(receiver).map(|holder| {
                                let key = self.cx.place_text(key).unwrap_or_else(|| "…".into());
                                format!("{holder}[{key}]")
                            });
                            Mode::Move(Destination::Place(place))
                        } else {
                            Mode::Read
                        };
                        self.expr(value, mode);
                    }
                    _ => self.call(callee, args, 1),
                }
            }
            K::New { def, args, .. } if Some(*def) == self.cx.weak_box => {
                for arg in args {
                    self.observe(arg);
                }
            }
            K::New { ctor, args, .. } => {
                for (i, arg) in args.iter().enumerate() {
                    let mode = match ctor {
                        Some(ctor) if self.cx.kept.contains(&(*ctor, i + 1)) => {
                            Mode::Move(self.cx.argument(*ctor, i + 1, Some(e.ty)))
                        }
                        Some(_) => Mode::Read,
                        None => Mode::Move(Destination::Aggregate),
                    };
                    self.expr(arg, mode);
                }
            }
            K::UnionNew { args, .. } => self.all(args, Mode::Move(Destination::Aggregate)),
            K::ArrayLit { elems, .. } | K::Tuple { elems } => {
                self.all(elems, Mode::Move(Destination::Aggregate))
            }
            K::InterfaceCall { receiver, args, .. } => {
                self.expr(receiver, Mode::Read);
                self.all(args, Mode::Read);
            }
            K::IndirectCall { target, args, .. } => {
                self.expr(target, Mode::Read);
                self.all(args, Mode::Read);
            }
            K::JsCall {
                target,
                via,
                method,
                args,
                ..
            } => {
                self.expr(target, Mode::Read);
                if let Some(via) = via {
                    self.expr(via, Mode::Read);
                }
                if let Some(method) = method {
                    self.expr(method, Mode::Read);
                }
                self.all(args, Mode::Read);
            }
            K::ArraySetUnchecked {
                array,
                index,
                value,
            } => {
                self.expr(array, Mode::Read);
                self.expr(index, Mode::Read);
                self.expr(value, Mode::Move(Destination::Place(None)));
            }
            K::RawStore { address, value, .. } => {
                self.expr(address, Mode::Read);
                self.expr(value, Mode::Move(Destination::Place(None)));
            }
            _ => {
                for child in crate::analyzer::cycles::visit::children(e) {
                    self.expr(child, Mode::Read);
                }
            }
        }
    }

    /// A value stored where it is only observed (a `weak` or `unowned` slot): its target, even
    /// when wrapped in `Some(...)`, stays with its owner.
    fn observe(&mut self, value: &HExpr) {
        match &value.kind {
            HExprKind::UnionNew { args, .. } => self.all(args, Mode::Read),
            _ => self.expr(value, Mode::Read),
        }
    }

    fn all(&mut self, exprs: &[HExpr], mode: Mode) {
        for e in exprs {
            self.expr(e, mode.clone());
        }
    }

    fn call(&mut self, callee: &Callee, args: &[HExpr], offset: usize) {
        for (i, arg) in args.iter().enumerate() {
            let param = i + offset;
            let mode = if self.cx.kept.contains(&(callee.def, param)) {
                Mode::Move(self.cx.argument(callee.def, param, None))
            } else {
                Mode::Read
            };
            self.expr(arg, mode);
        }
    }
}

impl Context<'_> {
    fn argument(
        &self,
        def: DefId,
        param: usize,
        constructed: Option<dream_types::TypeId>,
    ) -> Destination {
        let (callee, params) = self.function(def);
        let callee = match constructed {
            Some(ty) => self.type_name(ty),
            None => callee,
        };
        Destination::Argument {
            callee,
            param: params.get(param).cloned().unwrap_or_default(),
        }
    }
}
