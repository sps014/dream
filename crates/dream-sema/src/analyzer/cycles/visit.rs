//! Pre-order traversal of every expression in a HIR body, places included.

use dream_hir::{HExpr, HExprKind, HPattern, HPlace, HStmt};
use smallvec::SmallVec;

pub(super) fn stmts(body: &[HStmt], f: &mut impl FnMut(&HExpr)) {
    for stmt in body {
        self::stmt(stmt, f);
    }
}

fn stmt(s: &HStmt, f: &mut impl FnMut(&HExpr)) {
    match s {
        HStmt::Let { value, .. } | HStmt::Expr(value) => expr(value, f),
        HStmt::Assign { place, value } => {
            self::place(place, f);
            expr(value, f);
        }
        HStmt::Return(value) => {
            if let Some(value) = value {
                expr(value, f);
            }
        }
        HStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            expr(cond, f);
            stmts(then_branch, f);
            stmts(else_branch, f);
        }
        HStmt::While { cond, body, .. } | HStmt::DoWhile { cond, body, .. } => {
            expr(cond, f);
            stmts(body, f);
        }
        HStmt::For {
            init,
            cond,
            step,
            body,
            ..
        } => {
            stmts(init, f);
            expr(cond, f);
            stmts(step, f);
            stmts(body, f);
        }
        HStmt::Foreach { iterable, body, .. } => {
            expr(iterable, f);
            stmts(body, f);
        }
        HStmt::Lock { target, body } => {
            expr(target, f);
            stmts(body, f);
        }
        HStmt::Cleanup { body, cleanup } => {
            stmts(body, f);
            expr(cleanup, f);
        }
        HStmt::Defer { budget, body } => {
            if let Some(budget) = budget {
                expr(budget, f);
            }
            stmts(body, f);
        }
        HStmt::Switch {
            scrutinee,
            arms,
            default,
        } => {
            expr(scrutinee, f);
            for arm in arms {
                if let HPattern::Const(value) = &arm.pattern {
                    expr(value, f);
                }
                stmts(&arm.body, f);
            }
            stmts(default, f);
        }
        HStmt::Await { future, .. } => expr(future, f),
        HStmt::Break(_)
        | HStmt::Continue(_)
        | HStmt::DebugLine(_)
        | HStmt::SourceLine(_) => {}
    }
}

fn place(p: &HPlace, f: &mut impl FnMut(&HExpr)) {
    match p {
        HPlace::Local(_) | HPlace::Global(_) => {}
        HPlace::Field { obj, .. } => expr(obj, f),
        HPlace::Index { array, index } => {
            expr(array, f);
            expr(index, f);
        }
    }
}

pub(super) fn expr(e: &HExpr, f: &mut impl FnMut(&HExpr)) {
    f(e);
    for child in children(e) {
        expr(child, f);
    }
}

pub(super) type Children<'e> = SmallVec<[&'e HExpr; 4]>;

/// Direct subexpressions in evaluation order.
pub(super) fn children(e: &HExpr) -> Children<'_> {
    use HExprKind as K;
    let mut out = Children::new();
    match &e.kind {
        K::IntLit(_)
        | K::SizeOf(_)
        | K::AlignOf(_)
        | K::FloatLit(_)
        | K::BoolLit(_)
        | K::CharLit(_)
        | K::StringLit(_)
        | K::Var(_)
        | K::EnumValue(_)
        | K::Zero => {}
        K::Unary { operand: a, .. }
        | K::ArrayLen(a)
        | K::StrLen(a)
        | K::StrByteSize(a)
        | K::HashCode(a)
        | K::ToString(a)
        | K::ToBytes(a)
        | K::FromBytes(a)
        | K::WireEncode(a)
        | K::WireDecode(a)
        | K::RawLoad(a)
        | K::ForceFree(a)
        | K::Cast(a)
        | K::Await(a)
        | K::Discriminant(a)
        | K::EnumName { value: a, .. }
        | K::IsType { value: a, .. }
        | K::TypeName { value: a }
        | K::Print { arg: a, .. }
        | K::Field { obj: a, .. }
        | K::UnionField { base: a, .. }
        | K::ArrayNew { len: a, .. } => out.push(a),
        K::Binary { lhs: a, rhs: b, .. }
        | K::Index { array: a, index: b }
        | K::ArrayGetUnchecked { array: a, index: b }
        | K::ArraySlotTake {
            array: a, index: b, ..
        }
        | K::CharAt(a, b, _)
        | K::ByteAt(a, b)
        | K::Concat(a, b)
        | K::RawStore {
            address: a,
            value: b,
        }
        | K::ArrayRealloc {
            array: a,
            new_len: b,
            ..
        } => out.extend([&**a, &**b]),
        K::Call { args, .. } | K::UnionNew { args, .. } | K::New { args, .. } => out.extend(args),
        K::ArrayLit { elems, .. } | K::Tuple { elems } => out.extend(elems),
        K::MethodCall { receiver, args, .. } | K::InterfaceCall { receiver, args, .. } => {
            out.push(receiver);
            out.extend(args);
        }
        K::IndirectCall { target, args, .. } => {
            out.push(target);
            out.extend(args);
        }
        K::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => out.extend([&**dst, dst_off, src, src_off, count]),
        K::ArrayElemsFill {
            dst,
            dst_off,
            count,
            ..
        } => out.extend([&**dst, dst_off, count]),
        K::ArraySlotSwap {
            array,
            left,
            other_array,
            right,
            ..
        } => out.extend([&**array, left, other_array, right]),
        K::ArraySetUnchecked {
            array,
            index,
            value,
        } => out.extend([&**array, index, value]),
        K::Ternary {
            cond,
            then_expr,
            else_expr,
        } => out.extend([&**cond, then_expr, else_expr]),
        K::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            out.push(target);
            out.extend(via.as_deref());
            out.extend(method.as_deref());
            out.extend(args);
        }
    }
    out
}

