//! Walk a [`crate::relooper::Shape`] tree into nested C `for`/`if`/`switch`.
//!
//! Sync functions only. Async polls keep the existing PC dispatcher. If `reloop` returns `None`
//! (irreducible CFG), fall back to the label+goto walk.

use super::ast::{CTy, CaseKey, Expr, Stmt, SwitchArm, UnOp};
use super::builder::FuncBuilder;
use super::ctx::Cx;
use super::emit::Emitter;
use super::terminator::is_dense_switch;
use crate::relooper::{reloop, Shape};
use crate::{BlockId, Operand, Terminator};

#[derive(Clone)]
struct LoopFrame {
    header: BlockId,
    exit: Option<BlockId>,
    /// `continue` is valid only when the `for (;;)` body starts at `header`.
    continue_ok: bool,
}

struct ShapeEmit<'a> {
    cx: &'a Cx<'a>,
    f: &'a crate::MirFunction,
    b: &'a mut FuncBuilder,
    loops: Vec<LoopFrame>,
}

pub(super) fn emit_sync_body(cx: &Cx<'_>, f: &crate::MirFunction, b: &mut FuncBuilder) {
    if let Some(shape) = reloop(f) {
        if loops_are_single_header(&shape) {
            ShapeEmit {
                cx,
                f,
                b,
                loops: Vec::new(),
            }
            .shape(shape, None);
            return;
        }
    }
    b.goto(format!("L{}", f.entry.0));
    for (bi, block) in f.blocks.iter().enumerate() {
        b.label(format!("L{bi}"));
        let mut em = Emitter::new(cx, f, b);
        em.stmts(&block.stmts);
        em.term(&block.terminator);
    }
}

/// Multi-entry loops (`Loop` whose inner root is `Multiple`) cannot map `continue` onto one
/// C `for (;;)` header; those functions keep the label+goto walk.
fn loops_are_single_header(shape: &Shape) -> bool {
    match shape {
        Shape::Simple { next, .. } => next.as_deref().map(loops_are_single_header).unwrap_or(true),
        Shape::Loop { inner, next } => {
            matches!(inner.as_ref(), Shape::Simple { .. })
                && loops_are_single_header(inner)
                && next.as_deref().map(loops_are_single_header).unwrap_or(true)
        }
        Shape::Multiple { handled, next } => {
            handled.iter().all(loops_are_single_header)
                && next.as_deref().map(loops_are_single_header).unwrap_or(true)
        }
    }
}

impl ShapeEmit<'_> {
    fn shape(&mut self, shape: Shape, parent_ft: Option<BlockId>) {
        match shape {
            Shape::Simple { block, next } => {
                self.b.label(format!("L{}", block.0));
                let bb = self.f.block(block);
                {
                    let mut em = Emitter::new(self.cx, self.f, self.b);
                    em.stmts(&bb.stmts);
                }
                self.term(&bb.terminator, next, parent_ft);
            }
            Shape::Loop { inner, next } => {
                let header = shape_entry(&inner);
                let exit = next.as_ref().map(|n| shape_entry(n));
                // Only treat the header as fallthrough when it is the first statement in the
                // `for (;;)` body. A Multiple inner is a multi-entry loop; wrapping around to
                // the first arm would be the wrong header.
                let inner_ft = match inner.as_ref() {
                    Shape::Simple { block, .. } if *block == header => Some(header),
                    _ => None,
                };
                self.loops.push(LoopFrame {
                    header,
                    exit,
                    continue_ok: inner_ft.is_some(),
                });
                let body = self.nest_shape(*inner, inner_ft);
                self.loops.pop();
                self.b.stmt(Stmt::For {
                    init: Box::new(Stmt::Block(vec![])),
                    cond: Expr::i(1),
                    step: Box::new(Stmt::Block(vec![])),
                    body: Box::new(as_stmt(body)),
                });
                if let Some(n) = next {
                    self.shape(*n, parent_ft);
                }
            }
            Shape::Multiple { handled, next } => {
                // Arms are alternate entries, not sequential fallthrough. Using the join as
                // fallthrough would run the next arm after an Ok/Some path (dense computed
                // goto lands in the first label and continues into the second).
                for h in handled {
                    self.shape(h, None);
                }
                if let Some(n) = next {
                    self.shape(*n, parent_ft);
                }
            }
        }
    }

    fn term(&mut self, t: &Terminator, next: Option<Box<Shape>>, parent_ft: Option<BlockId>) {
        match t {
            Terminator::Goto(d) => {
                let ft = next.as_ref().map(|n| shape_entry(n)).or(parent_ft);
                self.transfer(*d, ft);
                if let Some(n) = next {
                    self.shape(*n, parent_ft);
                }
            }
            Terminator::If {
                cond,
                then_blk,
                else_blk,
            } => self.emit_if(cond, *then_blk, *else_blk, next, parent_ft),
            Terminator::Switch {
                value,
                targets,
                default,
            } => self.emit_switch(value, targets, *default, next, parent_ft),
            other => {
                let mut em = Emitter::new(self.cx, self.f, self.b);
                em.term(other);
            }
        }
    }

    fn emit_if(
        &mut self,
        cond: &Operand,
        then_blk: BlockId,
        else_blk: BlockId,
        next: Option<Box<Shape>>,
        parent_ft: Option<BlockId>,
    ) {
        let Some(n) = next else {
            self.push_if_transfers(cond, then_blk, else_blk, parent_ft);
            return;
        };
        match *n {
            Shape::Multiple {
                mut handled,
                next: join,
            } => {
                let join_id = join.as_ref().map(|j| shape_entry(j));
                let then_arm = take_arm(&mut handled, then_blk);
                let else_arm = take_arm(&mut handled, else_blk);
                if then_arm.is_some() || else_arm.is_some() {
                    let then_s = self.arm_stmts(then_arm, then_blk, join_id);
                    let else_s = self.arm_stmts(else_arm, else_blk, join_id);
                    self.push_if(cond, then_s, else_s);
                    for leftover in handled {
                        self.shape(leftover, join_id.or(parent_ft));
                    }
                    if let Some(j) = join {
                        self.shape(*j, parent_ft);
                    }
                    return;
                }
                let ft = join_id.or(parent_ft);
                self.push_if_transfers(cond, then_blk, else_blk, ft);
                for h in handled {
                    self.shape(h, ft);
                }
                if let Some(j) = join {
                    self.shape(*j, parent_ft);
                }
            }
            other => {
                let ft = Some(shape_entry(&other));
                self.push_if_transfers(cond, then_blk, else_blk, ft);
                self.shape(other, parent_ft);
            }
        }
    }

    fn arm_stmts(&mut self, arm: Option<Shape>, dest: BlockId, join: Option<BlockId>) -> Vec<Stmt> {
        if let Some(shape) = arm {
            self.nest_shape(shape, join)
        } else {
            self.nest_transfer(dest, join)
        }
    }

    fn push_if_transfers(
        &mut self,
        cond: &Operand,
        then_blk: BlockId,
        else_blk: BlockId,
        ft: Option<BlockId>,
    ) {
        let then_s = self.nest_transfer(then_blk, ft);
        let else_s = self.nest_transfer(else_blk, ft);
        self.push_if(cond, then_s, else_s);
    }

    fn push_if(&mut self, cond: &Operand, then_s: Vec<Stmt>, else_s: Vec<Stmt>) {
        let c = {
            let mut em = Emitter::new(self.cx, self.f, self.b);
            em.operand(cond)
        };
        match (then_s.is_empty(), else_s.is_empty()) {
            (true, true) => {}
            (true, false) => self.b.stmt(Stmt::if_(not_expr(c), as_stmt(else_s))),
            (false, true) => self.b.stmt(Stmt::if_(c, as_stmt(then_s))),
            (false, false) => self
                .b
                .stmt(Stmt::if_else(c, as_stmt(then_s), as_stmt(else_s))),
        }
    }

    fn emit_switch(
        &mut self,
        value: &Operand,
        targets: &[(i64, BlockId)],
        default: BlockId,
        next: Option<Box<Shape>>,
        parent_ft: Option<BlockId>,
    ) {
        let emit_raw = |s: &mut ShapeEmit<'_>, n: Option<Box<Shape>>| {
            {
                let mut em = Emitter::new(s.cx, s.f, s.b);
                em.term(&Terminator::Switch {
                    value: value.clone(),
                    targets: targets.to_vec(),
                    default,
                });
            }
            if let Some(rest) = n {
                s.shape(*rest, parent_ft);
            }
        };
        if is_dense_switch(targets) {
            emit_raw(self, next);
            return;
        }
        let Some(n) = next else {
            emit_raw(self, None);
            return;
        };
        let Shape::Multiple {
            mut handled,
            next: join,
        } = *n
        else {
            emit_raw(self, Some(n));
            return;
        };
        let join_id = join.as_ref().map(|j| shape_entry(j));
        let v = {
            let mut em = Emitter::new(self.cx, self.f, self.b);
            em.operand(value)
        };
        let mut arms = Vec::new();
        for (k, dest) in targets {
            let arm = take_arm(&mut handled, *dest);
            let mut body = self.arm_stmts(arm, *dest, join_id);
            if needs_switch_break(&body) {
                body.push(Stmt::Break);
            }
            arms.push(SwitchArm {
                keys: vec![CaseKey::Int(*k)],
                body,
            });
        }
        let def_arm = take_arm(&mut handled, default);
        let mut def_body = self.arm_stmts(def_arm, default, join_id);
        if needs_switch_break(&def_body) {
            def_body.push(Stmt::Break);
        }
        arms.push(SwitchArm {
            keys: vec![],
            body: def_body,
        });
        self.b.stmt(Stmt::Switch {
            expr: Expr::cast(CTy::I64, v),
            arms,
        });
        for leftover in handled {
            self.shape(leftover, join_id.or(parent_ft));
        }
        if let Some(j) = join {
            self.shape(*j, parent_ft);
        }
    }

    fn nest_shape(&mut self, shape: Shape, ft: Option<BlockId>) -> Vec<Stmt> {
        let loops = self.loops.clone();
        let cx = self.cx;
        let f = self.f;
        self.b.nested(|nb| {
            ShapeEmit {
                cx,
                f,
                b: nb,
                loops,
            }
            .shape(shape, ft);
        })
    }

    fn nest_transfer(&mut self, dest: BlockId, ft: Option<BlockId>) -> Vec<Stmt> {
        let loops = self.loops.clone();
        let cx = self.cx;
        let f = self.f;
        self.b.nested(|nb| {
            ShapeEmit {
                cx,
                f,
                b: nb,
                loops,
            }
            .transfer(dest, ft);
        })
    }

    fn transfer(&mut self, dest: BlockId, ft: Option<BlockId>) {
        if ft == Some(dest) {
            return;
        }
        for (i, lp) in self.loops.iter().enumerate().rev() {
            let innermost = i + 1 == self.loops.len();
            if dest == lp.header {
                if innermost && lp.continue_ok {
                    self.b.stmt(Stmt::Continue);
                } else {
                    self.b.goto(format!("L{}", dest.0));
                }
                return;
            }
            if lp.exit == Some(dest) {
                if innermost {
                    self.b.stmt(Stmt::Break);
                } else {
                    self.b.goto(format!("L{}", dest.0));
                }
                return;
            }
        }
        self.b.goto(format!("L{}", dest.0));
    }
}

fn shape_entry(shape: &Shape) -> BlockId {
    match shape {
        Shape::Simple { block, .. } => *block,
        Shape::Loop { inner, .. } => shape_entry(inner),
        Shape::Multiple { handled, next } => handled
            .first()
            .map(shape_entry)
            .or_else(|| next.as_ref().map(|n| shape_entry(n)))
            .unwrap_or(BlockId(0)),
    }
}

fn take_arm(handled: &mut Vec<Shape>, dest: BlockId) -> Option<Shape> {
    handled
        .iter()
        .position(|s| shape_entry(s) == dest)
        .map(|i| handled.remove(i))
}

fn as_stmt(mut v: Vec<Stmt>) -> Stmt {
    match v.len() {
        0 => Stmt::Block(vec![]),
        1 => v.pop().unwrap(),
        _ => Stmt::Block(v),
    }
}

fn not_expr(e: Expr) -> Expr {
    match e {
        Expr::Unary {
            op: UnOp::Not,
            expr,
        } => *expr,
        other => Expr::unary(UnOp::Not, other),
    }
}

fn needs_switch_break(body: &[Stmt]) -> bool {
    !matches!(
        body.last(),
        Some(
            Stmt::Return(_) | Stmt::Break | Stmt::Continue | Stmt::Goto(_) | Stmt::GotoIndirect(_)
        )
    )
}
