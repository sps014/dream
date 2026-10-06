//! `_into` reuse fusions: a string result assigned over a released old value is built by the
//! runtime's `_into` entry, which reuses the old block in place when it is uniquely owned.

use super::fx::{Fx, V};
use super::ir::{Ty, Value};
use crate::{Callee, Local, Operand, Place, Rvalue, Statement};
use dream_abi::intrinsics::IntrinsicOp;

impl<'l, 'a> Fx<'l, 'a> {
    fn is_substring_call(&self, callee: &Callee) -> bool {
        self.mir.intrinsics.iter().any(|(def, key)| {
            *def == callee.def && IntrinsicOp::from_key(key) == Some(IntrinsicOp::StringSubstring)
        })
    }

    fn is_into_rvalue(&self, rv: &Rvalue) -> bool {
        match rv {
            Rvalue::Concat(parts) => parts.len() == 2,
            Rvalue::ConcatInt { .. } => true,
            Rvalue::Call { callee, .. } => self.is_substring_call(callee),
            _ => false,
        }
    }

    /// `dest = rv` where `dest`'s old string is released first: the `_into` runtime entry reuses
    /// the old block in place when it is uniquely owned.
    fn emit_into(&mut self, dest: Local, rv: &Rvalue) {
        let slot = self.read_local(dest);
        let r = match rv {
            Rvalue::Concat(parts) if parts.len() == 2 => {
                let (a, b) = (self.operand(&parts[0]), self.operand(&parts[1]));
                self.call_v("dream_concat_strings_into", &[slot, a, b])
            }
            Rvalue::ConcatInt {
                prefix,
                value,
                suffix,
            } => {
                let p = self.operand(prefix);
                let v = self.operand(value);
                let v = self.conv_v(&v, &Ty::I32, false);
                let s = self.operand(suffix);
                self.call_v("dream_concat_str_int_str_into", &[slot, p, v, s])
            }
            Rvalue::Call { args, .. } => {
                let mut all = vec![slot];
                all.extend(args.iter().map(|a| self.operand(a)));
                self.call_v("dream_substring_into", &all)
            }
            _ => crate::internal_error!("into emit of non-reusable rvalue"),
        };
        self.store(&Place::Local(dest), rv, r);
    }

    pub(super) fn try_emit_into(&mut self, stmts: &[Statement], i: usize) -> Option<usize> {
        if i + 1 < stmts.len()
            && let (
                Statement::Release(Operand::Copy(Place::Local(rel))),
                Statement::Assign(Place::Local(dest), rv),
            ) = (&stmts[i], &stmts[i + 1])
            && rel.0 == dest.0
            && self.is_into_rvalue(rv)
            && !crate::passes::rvalue_reads_local(rv, dest.0)
        {
            self.emit_into(*dest, rv);
            return Some(2);
        }
        if i + 2 < stmts.len()
            && let (
                Statement::Assign(Place::Local(tmp), rv),
                Statement::Release(Operand::Copy(Place::Local(rel))),
                Statement::Assign(
                    Place::Local(dest),
                    Rvalue::Use(Operand::Copy(Place::Local(src))),
                ),
            ) = (&stmts[i], &stmts[i + 1], &stmts[i + 2])
            && src.0 == tmp.0
            && rel.0 == dest.0
            && tmp.0 != dest.0
            && self.is_into_rvalue(rv)
            && !crate::passes::rvalue_reads_local(rv, dest.0)
        {
            self.emit_into(*dest, rv);
            let v = self.read_local(*dest);
            self.write_local(*tmp, &v);
            return Some(3);
        }
        self.try_emit_into_chain(stmts, i)
    }

    /// `t0 = rv`, then the result handed down a chain of temporaries (copies, moves and null
    /// stores of chain members only) before `release dest; dest = t_last`. RC insertion and
    /// inlining leave this shape around call results, so the plain three-statement match misses it.
    fn try_emit_into_chain(&mut self, stmts: &[Statement], i: usize) -> Option<usize> {
        let Statement::Assign(Place::Local(t0), rv) = &stmts[i] else {
            return None;
        };
        if !self.is_into_rvalue(rv) {
            return None;
        }
        let mut chain: Vec<(Local, bool)> = vec![(*t0, true)];
        let mut cur = *t0;
        let mut j = i + 1;
        let dest = loop {
            match stmts.get(j)? {
                Statement::Assign(
                    Place::Local(t),
                    Rvalue::Use(Operand::Const(crate::Const::Null)),
                ) if chain.iter().any(|(c, _)| c == t) && *t != cur => {
                    chain
                        .iter_mut()
                        .filter(|(c, _)| c == t)
                        .for_each(|e| e.1 = false);
                }
                Statement::Assign(Place::Local(t), next)
                    if chain_source(next) == Some(cur) && !chain.iter().any(|(c, _)| c == t) =>
                {
                    if matches!(next, Rvalue::Move { .. }) {
                        chain
                            .iter_mut()
                            .filter(|(c, _)| *c == cur)
                            .for_each(|e| e.1 = false);
                    }
                    chain.push((*t, true));
                    cur = *t;
                }
                Statement::Release(Operand::Copy(Place::Local(rel)))
                    if !chain.iter().any(|(c, _)| c == rel) =>
                {
                    break *rel;
                }
                _ => return None,
            }
            j += 1;
        };
        let Some(Statement::Assign(Place::Local(d), last)) = stmts.get(j + 1) else {
            return None;
        };
        if *d != dest
            || chain_source(last) != Some(cur)
            || crate::passes::rvalue_reads_local(rv, dest.0)
        {
            return None;
        }
        if matches!(last, Rvalue::Move { .. }) {
            chain
                .iter_mut()
                .filter(|(c, _)| *c == cur)
                .for_each(|e| e.1 = false);
        }
        self.emit_into(dest, rv);
        let v = self.read_local(dest);
        for (t, holds) in chain {
            let val = if holds {
                v.clone()
            } else {
                V::s(Value::zero(self.h()))
            };
            self.write_local(t, &val);
        }
        Some(j + 2 - i)
    }
}

fn chain_source(rv: &Rvalue) -> Option<Local> {
    match rv {
        Rvalue::Use(Operand::Copy(Place::Local(s))) | Rvalue::Move { src: s, cast: None } => {
            Some(*s)
        }
        _ => None,
    }
}
