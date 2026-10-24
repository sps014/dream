//! Statement transfer: assignments, structured control flow, and loop fixpoints.

use super::exprs::CLOSURE_ENV;
use super::state::State;
use super::summary::Origin;
use super::value::{Atom, Atoms, Slot, one, union};
use super::walker::{LoopFrame, Walker};
use dream_hir::{Binding, HExpr, HExprKind, HPattern, HPlace, HStmt};
use dream_types::TypeId;

/// Loops that have not stabilized after this many passes are widened.
const MAX_LOOP_PASSES: usize = 12;

impl Walker<'_, '_> {
    pub(super) fn block(&mut self, body: &[HStmt]) {
        for stmt in body {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &HStmt) {
        match stmt {
            HStmt::Let { local, ty, value } => {
                let value = self.eval(value);
                self.bind(local.0, *ty, value);
            }
            HStmt::Assign { place, value } => self.assign(place, value),
            HStmt::Expr(value) => {
                self.eval(value);
            }
            HStmt::Await { future, .. } => {
                self.eval(future);
                self.suspend();
            }
            HStmt::Return(value) => {
                if let Some(value) = value {
                    let value = self.eval(value);
                    union(&mut self.summary.ret, &value);
                }
                let state = self.state.clone();
                match &mut self.exit {
                    Some(exit) => exit.join(&state),
                    None => self.exit = Some(state),
                }
            }
            HStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                self.eval(cond);
                let before = self.state.clone();
                self.block(then_branch);
                let then_state = std::mem::replace(&mut self.state, before);
                self.block(else_branch);
                self.state.join(&then_state);
            }
            HStmt::While { cond, body, label } => self.run_loop(label, |w| {
                w.eval(cond);
                w.block(body);
                w.join_continues();
            }),
            HStmt::DoWhile { cond, body, label } => self.run_loop(label, |w| {
                w.block(body);
                w.join_continues();
                w.eval(cond);
            }),
            HStmt::For {
                init,
                cond,
                step,
                body,
                label,
            } => {
                self.block(init);
                self.run_loop(label, |w| {
                    w.eval(cond);
                    w.block(body);
                    w.join_continues();
                    w.block(step);
                });
            }
            HStmt::Foreach {
                elem,
                iterable,
                body,
                label,
            } => {
                let items = self.eval(iterable);
                let elem_ty = self.local_type(elem.0);
                self.run_loop(label, |w| {
                    let item = match w.prog.graph.interner().kind(iterable.ty) {
                        dream_types::TyKind::Array(_) => w.read(&items, Slot::Elem(iterable.ty)),
                        _ if items.is_empty() => Atoms::new(),
                        _ => one(Atom::Old),
                    };
                    w.bind(elem.0, elem_ty.unwrap_or(iterable.ty), item);
                    w.block(body);
                    w.join_continues();
                });
            }
            HStmt::Lock { target, body } => {
                self.eval(target);
                self.block(body);
            }
            HStmt::Cleanup { body, cleanup } => {
                self.block(body);
                self.eval(cleanup);
            }
            HStmt::Defer { budget, body } => {
                if let Some(budget) = budget {
                    self.eval(budget);
                }
                // The body runs at scope exit, after stores this walk has not seen yet.
                let resume = self.state.clone();
                self.state.demote_all(self.paths);
                self.block(body);
                self.state.join(&resume);
            }
            HStmt::Switch {
                scrutinee,
                arms,
                default,
            } => {
                let value = self.eval(scrutinee);
                let before = self.state.clone();
                let mut merged: Option<State> = None;
                for arm in arms {
                    self.state = before.clone();
                    match &arm.pattern {
                        HPattern::Const(constant) => {
                            self.eval(constant);
                        }
                        HPattern::Variant {
                            variant, bindings, ..
                        } => {
                            for (i, local) in bindings.iter().enumerate() {
                                let payload =
                                    self.payload(&value, scrutinee.ty, *variant as u32, i as u32);
                                let ty = self.local_type(local.0).unwrap_or(scrutinee.ty);
                                self.bind(local.0, ty, payload);
                            }
                        }
                        HPattern::Wildcard => {}
                    }
                    self.block(&arm.body);
                    merge(&mut merged, &self.state);
                }
                self.state = before;
                self.block(default);
                merge(&mut merged, &self.state);
                self.state = merged.unwrap_or_default();
            }
            HStmt::Break(label) => {
                let state = self.state.clone();
                if let Some(frame) = self.frame(label) {
                    merge(&mut frame.breaks, &state);
                }
            }
            HStmt::Continue(label) => {
                let state = self.state.clone();
                if let Some(frame) = self.frame(label) {
                    merge(&mut frame.continues, &state);
                }
            }
            HStmt::SourceLine(line) => self.line = *line,
            HStmt::DebugLine(_) => {}
        }
    }

    pub(super) fn bind(&mut self, local: u32, ty: TypeId, value: Atoms) {
        let keep = self.bearing(ty) || self.local_type(local).is_none();
        self.state
            .set_local(local as usize, if keep { value } else { Atoms::new() });
    }

    fn frame(&mut self, label: &Option<String>) -> Option<&mut LoopFrame> {
        let index = match label {
            Some(label) => self
                .loops
                .iter()
                .rposition(|f| f.label.as_deref() == Some(label))
                .or(self.loops.len().checked_sub(1)),
            None => self.loops.len().checked_sub(1),
        }?;
        self.loops.get_mut(index)
    }

    fn join_continues(&mut self) {
        if let Some(frame) = self.loops.last_mut()
            && let Some(continues) = frame.continues.take()
        {
            self.state.join(&continues);
        }
    }

    fn run_loop(&mut self, label: &Option<String>, mut pass: impl FnMut(&mut Self)) {
        self.loops.push(LoopFrame {
            label: label.clone(),
            breaks: None,
            continues: None,
        });
        let mut stable = false;
        for _ in 0..MAX_LOOP_PASSES {
            let head = self.state.clone();
            pass(self);
            self.state.join(&head);
            if self.state == head {
                stable = true;
                break;
            }
        }
        if !stable {
            self.state.widen(self.paths);
            let head = self.state.clone();
            pass(self);
            self.state.join(&head);
        }
        if let Some(breaks) = self.loops.pop().and_then(|frame| frame.breaks) {
            self.state.join(&breaks);
        }
    }

    fn assign(&mut self, place: &HPlace, value: &HExpr) {
        match place {
            HPlace::Local(local) => {
                let atoms = self.eval(value);
                if self.ref_locals.contains(&local.0) {
                    self.store(&[Atom::Old], None, &atoms, None);
                }
                let ty = self.local_type(local.0).unwrap_or(value.ty);
                self.bind(local.0, ty, atoms);
            }
            HPlace::Global(global) if *global == CLOSURE_ENV => {
                self.closure_env = self.eval(value);
            }
            HPlace::Global(_) => {
                let atoms = self.eval(value);
                self.retain(&atoms);
            }
            HPlace::Field { obj, field } => {
                let mut inline = Vec::new();
                let (mut targets, container) = self.container(obj, &mut inline);
                let atoms = self.eval_holding(&mut targets, value);
                let weak = self.is_weak_field(obj.ty, *field);
                let slot = self.field_slot(obj.ty, *field);
                if weak {
                    self.store(&targets, Some(slot), &atoms, None);
                } else {
                    let origin = Origin {
                        container,
                        slot,
                        value: value.ty,
                    };
                    let check = self.prog.graph.relevant(container, value.ty);
                    self.publishing = true;
                    self.store(&targets, Some(slot), &atoms, check.then_some(origin));
                    self.publishing = false;
                }
                self.wrote(slot, &targets);
                let exact = inline.is_empty();
                for slot in inline {
                    self.wrote(slot, &targets);
                }
                if exact
                    && let [target @ Atom::Param(_, path)] = targets[..]
                    && path.is_exact()
                {
                    self.state.set_known(target, slot, atoms);
                }
                self.published_now.clear();
            }
            HPlace::Index { array, index } => {
                let mut operands = self.eval_all([array, index, value].into_iter());
                let atoms = std::mem::take(&mut operands[2]);
                let targets = std::mem::take(&mut operands[0]);
                let slot = Slot::Elem(array.ty);
                let origin = Origin {
                    container: array.ty,
                    slot,
                    value: value.ty,
                };
                let check = self.prog.graph.relevant(array.ty, value.ty);
                self.publishing = true;
                self.store(&targets, Some(slot), &atoms, check.then_some(origin));
                self.publishing = false;
                self.wrote(slot, &targets);
                self.published_now.clear();
            }
        }
    }

    /// The heap object a field store writes into. Inline value-struct fields belong to the
    /// object (or local aggregate) that contains them.
    fn container(&mut self, obj: &HExpr, inline: &mut Vec<Slot>) -> (Atoms, TypeId) {
        let interner = self.prog.graph.interner();
        if !interner.is_value_type(obj.ty) {
            return (self.eval(obj), obj.ty);
        }
        match &obj.kind {
            HExprKind::Field { obj: inner, field } => {
                inline.push(self.field_slot(inner.ty, *field));
                self.container(inner, inline)
            }
            HExprKind::Index { array, index } => {
                let mut targets = self.eval(array);
                self.eval_holding(&mut targets, index);
                inline.push(Slot::Elem(array.ty));
                (targets, array.ty)
            }
            HExprKind::Var(Binding::Local(local)) => {
                let mut atoms = self.state.local(local.0 as usize);
                if atoms.is_empty() && self.bearing(obj.ty) {
                    atoms = one(self.site(obj as *const HExpr as usize, 0));
                    self.state.set_local(local.0 as usize, atoms.clone());
                }
                (atoms, obj.ty)
            }
            _ => (self.eval(obj), obj.ty),
        }
    }

    pub(super) fn is_weak_field(&self, owner: TypeId, field: usize) -> bool {
        self.prog
            .graph
            .layouts()
            .structs
            .get(&owner)
            .and_then(|layout| layout.fields.get(field))
            .is_some_and(|f| f.is_weak || f.is_unowned)
    }

    pub(super) fn field_slot(&self, owner: TypeId, field: usize) -> Slot {
        if self.is_weak_field(owner, field) {
            Slot::Weak(owner, field as u32)
        } else {
            Slot::Field(owner, field as u32)
        }
    }
}

fn merge(slot: &mut Option<State>, state: &State) {
    match slot {
        Some(existing) => existing.join(state),
        None => *slot = Some(state.clone()),
    }
}
