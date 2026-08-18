//! The only C syntax writer. Emitters construct [`super::ast`] nodes instead.

use super::ast::{CTy, CaseKey, Expr, Func, Item, Param, Stmt, UnOp, Unit};
use crate::BinOp;

pub fn print_unit(unit: &Unit) -> String {
    let mut out = String::new();
    let mut last_include = false;
    for item in &unit.items {
        match item {
            Item::Include(h) => {
                out.push_str("#include ");
                out.push_str(h);
                out.push('\n');
                last_include = true;
            }
            other => {
                if last_include {
                    out.push('\n');
                    last_include = false;
                }
                print_item(&mut out, other);
            }
        }
    }
    out
}

fn print_item(out: &mut String, item: &Item) {
    match item {
        Item::Include(_) => {}
        Item::Global {
            thread_local,
            align,
            static_,
            const_,
            ty,
            name,
            init,
        } => {
            if *thread_local {
                out.push_str("_Thread_local ");
            }
            if let Some(a) = align {
                out.push_str(&format!("_Alignas({a}) "));
            }
            if *static_ {
                out.push_str("static ");
            }
            if *const_ {
                out.push_str("const ");
            }
            print_decl_ty(out, ty, name);
            if let Some(e) = init {
                out.push_str(" = ");
                print_expr(out, e, 0);
            }
            out.push_str(";\n");
        }
        Item::Proto {
            static_,
            ret,
            name,
            params,
        } => {
            if *static_ {
                out.push_str("static ");
            }
            print_ty(out, ret);
            out.push(' ');
            out.push_str(name);
            print_params(out, params);
            out.push_str(";\n");
        }
        Item::Func(f) => {
            print_func(out, f);
            out.push('\n');
        }
    }
}

fn print_func(out: &mut String, f: &Func) {
    if let Some(attr) = f.attr {
        out.push_str(attr);
        out.push(' ');
    }
    if f.static_ {
        out.push_str("static ");
    }
    print_ty(out, &f.ret);
    out.push(' ');
    out.push_str(&f.name);
    print_params(out, &f.params);
    out.push_str(" {\n");
    for s in &f.body {
        print_stmt(out, s, 1, true);
    }
    out.push_str("}\n");
}

fn print_params(out: &mut String, params: &[Param]) {
    out.push('(');
    if params.is_empty() {
        out.push_str("void");
    } else {
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            print_decl_ty(out, &p.ty, &p.name);
        }
    }
    out.push(')');
}

fn print_ty(out: &mut String, ty: &CTy) {
    match ty {
        CTy::Void => out.push_str("void"),
        CTy::U8 => out.push_str("uint8_t"),
        CTy::U16 => out.push_str("uint16_t"),
        CTy::I32 => out.push_str("int32_t"),
        CTy::U32 => out.push_str("uint32_t"),
        CTy::Unsigned => out.push_str("unsigned"),
        CTy::I64 => out.push_str("int64_t"),
        CTy::F32 => out.push_str("float"),
        CTy::F64 => out.push_str("double"),
        CTy::Ptr => out.push_str("dream_ptr"),
        CTy::VoidPtr => out.push_str("void *"),
        CTy::CharPtr => out.push_str("char *"),
        CTy::PtrTo(inner) => {
            print_ty(out, inner);
            out.push_str(" *");
        }
        CTy::Array { elem, .. } => print_ty(out, elem),
        CTy::Named(s) => out.push_str(s),
        CTy::Ident(s) => out.push_str(s),
        CTy::Struct { fields } => {
            out.push_str("struct { ");
            for (ty, name) in fields {
                print_decl_ty(out, ty, name);
                out.push_str("; ");
            }
            out.push('}');
        }
    }
}

fn print_decl_ty(out: &mut String, ty: &CTy, name: &str) {
    match ty {
        CTy::Array { elem, len } => {
            print_ty(out, elem);
            out.push(' ');
            out.push_str(name);
            out.push('[');
            out.push_str(&len.to_string());
            out.push(']');
        }
        CTy::PtrTo(inner) => {
            print_ty(out, inner);
            out.push_str(" *");
            out.push_str(name);
        }
        CTy::VoidPtr => {
            out.push_str("void *");
            out.push_str(name);
        }
        CTy::CharPtr => {
            out.push_str("char *");
            out.push_str(name);
        }
        _ => {
            print_ty(out, ty);
            out.push(' ');
            out.push_str(name);
        }
    }
}

fn indent(out: &mut String, n: usize) {
    for _ in 0..n {
        out.push_str("  ");
    }
}

fn print_stmt(out: &mut String, stmt: &Stmt, ind: usize, nl: bool) {
    match stmt {
        Stmt::Label(name) => {
            out.push_str(name);
            out.push_str(":;\n");
        }
        Stmt::Block(stmts) => {
            indent(out, ind);
            out.push_str("{\n");
            for s in stmts {
                print_stmt(out, s, ind + 1, true);
            }
            indent(out, ind);
            out.push('}');
            if nl {
                out.push('\n');
            }
        }
        Stmt::If {
            cond,
            then_s,
            else_s,
        } => {
            indent(out, ind);
            out.push_str("if (");
            print_expr(out, cond, 0);
            out.push_str(") ");
            print_stmt_inline(out, then_s, ind);
            if let Some(e) = else_s {
                out.push_str(" else ");
                print_stmt_inline(out, e, ind);
            }
            if nl {
                out.push('\n');
            }
        }
        Stmt::Switch { expr, arms } => {
            indent(out, ind);
            out.push_str("switch (");
            print_expr(out, expr, 0);
            out.push_str(") {\n");
            for arm in arms {
                if arm.keys.is_empty() {
                    indent(out, ind + 1);
                    out.push_str("default:");
                } else {
                    for (i, k) in arm.keys.iter().enumerate() {
                        indent(out, ind + 1);
                        out.push_str("case ");
                        match k {
                            CaseKey::Int(n) => out.push_str(&n.to_string()),
                            CaseKey::Ident(s) => out.push_str(s),
                        }
                        out.push(':');
                        if i + 1 < arm.keys.len() {
                            out.push('\n');
                        }
                    }
                }
                if arm.body.is_empty() {
                    out.push('\n');
                    continue;
                }
                if arm.body.len() == 1 && !matches!(arm.body[0], Stmt::Block(_) | Stmt::Switch { .. } | Stmt::If { .. } | Stmt::For { .. }) {
                    out.push(' ');
                    print_stmt_inline(out, &arm.body[0], ind + 1);
                    out.push('\n');
                } else {
                    out.push('\n');
                    for s in &arm.body {
                        print_stmt(out, s, ind + 2, true);
                    }
                }
            }
            indent(out, ind);
            out.push('}');
            if nl {
                out.push('\n');
            }
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => {
            indent(out, ind);
            out.push_str("for (");
            print_for_clause(out, init);
            out.push_str("; ");
            print_expr(out, cond, 0);
            out.push_str("; ");
            print_for_clause(out, step);
            out.push_str(") ");
            print_stmt_inline(out, body, ind);
            if nl {
                out.push('\n');
            }
        }
        Stmt::Goto(l) => {
            indent(out, ind);
            out.push_str("goto ");
            out.push_str(l);
            out.push(';');
            if nl {
                out.push('\n');
            }
        }
        Stmt::GotoIndirect(e) => {
            indent(out, ind);
            out.push_str("goto *");
            print_expr(out, e, 14);
            out.push(';');
            if nl {
                out.push('\n');
            }
        }
        Stmt::Return(None) => {
            indent(out, ind);
            out.push_str("return;");
            if nl {
                out.push('\n');
            }
        }
        Stmt::Return(Some(e)) => {
            indent(out, ind);
            out.push_str("return ");
            print_expr(out, e, 0);
            out.push(';');
            if nl {
                out.push('\n');
            }
        }
        Stmt::Decl {
            align,
            static_,
            const_,
            ty,
            name,
            init,
        } => {
            indent(out, ind);
            if let Some(a) = align {
                out.push_str(&format!("_Alignas({a}) "));
            }
            if *static_ {
                out.push_str("static ");
            }
            if *const_ {
                out.push_str("const ");
            }
            print_decl_ty(out, ty, name);
            if let Some(e) = init {
                out.push_str(" = ");
                print_expr(out, e, 0);
            }
            out.push(';');
            if nl {
                out.push('\n');
            }
        }
        Stmt::Assign { dest, src } => {
            indent(out, ind);
            print_expr(out, dest, 1);
            out.push_str(" = ");
            print_expr(out, src, 0);
            out.push(';');
            if nl {
                out.push('\n');
            }
        }
        Stmt::Expr(e) => {
            indent(out, ind);
            print_expr(out, e, 0);
            out.push(';');
            if nl {
                out.push('\n');
            }
        }
    }
}

fn print_stmt_inline(out: &mut String, stmt: &Stmt, ind: usize) {
    match stmt {
        Stmt::Block(_) | Stmt::Switch { .. } | Stmt::For { .. } => {
            let start = out.len();
            print_stmt(out, stmt, ind, false);
            if out[start..].starts_with("  ") {
                let stripped: String = out[start..].trim_start().to_string();
                out.truncate(start);
                out.push_str(&stripped);
            }
        }
        Stmt::If { .. } => print_stmt(out, stmt, 0, false),
        _ => {
            let start = out.len();
            print_stmt(out, stmt, 0, false);
            if out[start..].starts_with("  ") {
                let stripped: String = out[start..].trim_start().to_string();
                out.truncate(start);
                out.push_str(&stripped);
            }
        }
    }
}

fn print_for_clause(out: &mut String, stmt: &Stmt) {
    match stmt {
        Stmt::Assign { dest, src } => {
            print_expr(out, dest, 1);
            out.push_str(" = ");
            print_expr(out, src, 0);
        }
        Stmt::Expr(Expr::PostInc(e)) => {
            print_expr(out, e, 14);
            out.push_str("++");
        }
        Stmt::Expr(e) => print_expr(out, e, 0),
        Stmt::Decl {
            ty, name, init, ..
        } => {
            print_decl_ty(out, ty, name);
            if let Some(e) = init {
                out.push_str(" = ");
                print_expr(out, e, 0);
            }
        }
        _ => print_stmt(out, stmt, 0, false),
    }
}

fn bin_prec(op: BinOp) -> u8 {
    match op {
        BinOp::Mul | BinOp::Div | BinOp::Rem => 12,
        BinOp::Add | BinOp::Sub => 11,
        BinOp::Shl | BinOp::Shr => 10,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 9,
        BinOp::Eq | BinOp::Ne => 8,
        BinOp::BitAnd => 7,
        BinOp::BitXor => 6,
        BinOp::BitOr => 5,
        BinOp::And => 4,
        BinOp::Or => 3,
    }
}

fn bin_sym(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
    }
}

fn print_expr(out: &mut String, expr: &Expr, prec: u8) {
    match expr {
        Expr::Ident(s) => out.push_str(s),
        Expr::Int(n) => out.push_str(&n.to_string()),
        Expr::UInt(n) => {
            out.push_str(&n.to_string());
            out.push('u');
        }
        Expr::Long(n) => {
            out.push_str(&n.to_string());
            out.push_str("LL");
        }
        Expr::Float(v) => print_f64(out, *v),
        Expr::F32(v) => print_f32(out, *v),
        Expr::Null => out.push('0'),
        Expr::Nan { double } => {
            if *double {
                out.push_str("(double)NAN");
            } else {
                out.push_str("NAN");
            }
        }
        Expr::Inf { double, neg } => {
            let inf = if *double {
                "(double)INFINITY"
            } else {
                "INFINITY"
            };
            if *neg {
                out.push_str("(-");
                out.push_str(inf);
                out.push(')');
            } else {
                out.push_str(inf);
            }
        }
        Expr::Unary { op, expr } => {
            paren(out, prec, 13, |out| {
                out.push(match op {
                    UnOp::Neg => '-',
                    UnOp::Not => '!',
                    UnOp::BitNot => '~',
                });
                print_expr(out, expr, 13);
            });
        }
        Expr::Binary { op, lhs, rhs } => {
            let p = bin_prec(*op);
            paren(out, prec, p, |out| {
                print_expr(out, lhs, p);
                out.push(' ');
                out.push_str(bin_sym(*op));
                out.push(' ');
                print_expr(out, rhs, p + 1);
            });
        }
        Expr::Ternary {
            cond,
            then_e,
            else_e,
        } => {
            paren(out, prec, 2, |out| {
                print_expr(out, cond, 3);
                out.push_str(" ? ");
                print_expr(out, then_e, 2);
                out.push_str(" : ");
                print_expr(out, else_e, 2);
            });
        }
        Expr::Call { name, args } => {
            out.push_str(name);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(out, a, 1);
            }
            out.push(')');
        }
        Expr::IndirectCall { callee, args } => {
            out.push('(');
            print_expr(out, callee, 14);
            out.push(')');
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(out, a, 1);
            }
            out.push(')');
        }
        Expr::Cast { ty, expr } => {
            paren(out, prec, 13, |out| {
                out.push('(');
                print_ty(out, ty);
                out.push(')');
                print_expr(out, expr, 13);
            });
        }
        Expr::Deref(e) => {
            paren(out, prec, 13, |out| {
                out.push('*');
                print_expr(out, e, 13);
            });
        }
        Expr::AddrOf(e) => {
            paren(out, prec, 13, |out| {
                out.push('&');
                print_expr(out, e, 13);
            });
        }
        Expr::Index { base, index } => {
            print_expr(out, base, 14);
            out.push('[');
            print_expr(out, index, 0);
            out.push(']');
        }
        Expr::Comma(a, b) => {
            paren(out, prec, 1, |out| {
                print_expr(out, a, 1);
                out.push_str(", ");
                print_expr(out, b, 1);
            });
        }
        Expr::PostInc(e) => {
            paren(out, prec, 14, |out| {
                print_expr(out, e, 14);
                out.push_str("++");
            });
        }
        Expr::Compound(elems) => {
            out.push('{');
            for (i, e) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(out, e, 0);
            }
            if elems.is_empty() {
                out.push('0');
            }
            out.push('}');
        }
        Expr::CompoundTyped { ty, elems } => {
            out.push('(');
            print_ty(out, ty);
            out.push(')');
            out.push('{');
            for (i, e) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(out, e, 0);
            }
            out.push('}');
        }
        Expr::LabelAddr(l) => {
            out.push_str("&&");
            out.push_str(l);
        }
        Expr::Gnu { stmts, result } => {
            out.push_str("({ ");
            for s in stmts {
                print_stmt_compact(out, s);
                out.push(' ');
            }
            print_expr(out, result, 0);
            out.push_str("; })");
        }
    }
}

fn print_stmt_compact(out: &mut String, stmt: &Stmt) {
    match stmt {
        Stmt::Decl {
            ty, name, init, ..
        } => {
            print_decl_ty(out, ty, name);
            if let Some(e) = init {
                out.push_str(" = ");
                print_expr(out, e, 0);
            }
            out.push(';');
        }
        Stmt::Assign { dest, src } => {
            print_expr(out, dest, 1);
            out.push_str(" = ");
            print_expr(out, src, 0);
            out.push(';');
        }
        Stmt::Expr(e) => {
            print_expr(out, e, 0);
            out.push(';');
        }
        Stmt::If {
            cond,
            then_s,
            else_s,
        } => {
            out.push_str("if (");
            print_expr(out, cond, 0);
            out.push_str(") ");
            print_stmt_compact(out, then_s);
            if let Some(e) = else_s {
                out.push_str(" else ");
                print_stmt_compact(out, e);
            }
        }
        Stmt::Block(stmts) => {
            out.push_str("{ ");
            for s in stmts {
                print_stmt_compact(out, s);
                out.push(' ');
            }
            out.push('}');
        }
        Stmt::Return(None) => out.push_str("return;"),
        Stmt::Return(Some(e)) => {
            out.push_str("return ");
            print_expr(out, e, 0);
            out.push(';');
        }
        other => {
            let mut tmp = String::new();
            print_stmt(&mut tmp, other, 0, false);
            out.push_str(tmp.trim());
        }
    }
}

fn paren(out: &mut String, outer: u8, inner: u8, f: impl FnOnce(&mut String)) {
    if inner < outer {
        out.push('(');
        f(out);
        out.push(')');
    } else {
        f(out);
    }
}

fn print_f64(out: &mut String, v: f64) {
    if v.is_nan() {
        out.push_str("(double)NAN");
    } else if v.is_infinite() {
        if v > 0.0 {
            out.push_str("(double)INFINITY");
        } else {
            out.push_str("(-(double)INFINITY)");
        }
    } else {
        let mut s = format!("{v}");
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            s.push_str(".0");
        }
        out.push_str(&s);
    }
}

fn print_f32(out: &mut String, v: f32) {
    if v.is_nan() {
        out.push_str("NAN");
    } else if v.is_infinite() {
        if v > 0.0 {
            out.push_str("INFINITY");
        } else {
            out.push_str("(-INFINITY)");
        }
    } else {
        let mut s = format!("{v}");
        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
            s.push_str(".0");
        }
        s.push('f');
        out.push_str(&s);
    }
}
