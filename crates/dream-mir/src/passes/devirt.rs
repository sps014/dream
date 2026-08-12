//! Closed-world interface devirtualization: a unique implementor (or a unique method symbol for
//! a slot) becomes a direct [`Rvalue::Call`] / [`Statement::Call`] the inliner can eat.

use super::ModulePass;
use crate::{Callee, Rvalue, Statement};
use dream_types::TypeInterner;
use std::collections::HashMap;

pub struct Devirt;

impl ModulePass for Devirt {
    fn name(&self) -> &'static str {
        "devirt"
    }

    fn run(&self, mir: &mut crate::Mir, _interner: &TypeInterner) -> bool {
        let map = unique_slot_callees(mir);
        if map.is_empty() {
            return false;
        }
        let mut changed = false;
        for f in &mut mir.functions {
            for block in &mut f.blocks {
                for stmt in &mut block.stmts {
                    changed |= rewrite_stmt(stmt, &map);
                }
            }
        }
        changed
    }
}

fn unique_slot_callees(mir: &crate::Mir) -> HashMap<(usize, usize), Callee> {
    // (iface, slot) -> unique concrete WAT/MIR name, or None if conflicting.
    let mut names: HashMap<(usize, usize), Option<String>> = HashMap::new();
    for impl_ in &mir.interfaces.impls {
        for (iface_id, slots) in &impl_.entries {
            for (slot, sym) in slots.iter().enumerate() {
                let key = (*iface_id, slot);
                match names.get(&key) {
                    None => {
                        names.insert(key, Some(sym.clone()));
                    }
                    Some(Some(prev)) if prev == sym => {}
                    Some(_) => {
                        names.insert(key, None);
                    }
                }
            }
        }
    }
    let mut by_name: HashMap<&str, &crate::MirFunction> = HashMap::new();
    for f in &mir.functions {
        by_name.insert(f.name.as_str(), f);
    }
    let mut out = HashMap::new();
    for (key, name) in names {
        let Some(name) = name else { continue };
        let Some(f) = by_name.get(name.as_str()) else {
            continue;
        };
        out.insert(
            key,
            Callee {
                def: f.def,
                args: f.instance.clone(),
                ret: f.ret,
            },
        );
    }
    out
}

fn rewrite_stmt(stmt: &mut Statement, map: &HashMap<(usize, usize), Callee>) -> bool {
    match stmt {
        Statement::InterfaceCall {
            receiver,
            iface_id,
            method_slot,
            args,
            ..
        } => {
            let Some(callee) = map.get(&(*iface_id, *method_slot)) else {
                return false;
            };
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(receiver.clone());
            call_args.extend(args.iter().cloned());
            *stmt = Statement::Call {
                callee: callee.clone(),
                args: call_args,
            };
            true
        }
        Statement::Assign(place, Rvalue::InterfaceCall {
            receiver,
            iface_id,
            method_slot,
            args,
            ret,
            ..
        }) => {
            let Some(mut callee) = map.get(&(*iface_id, *method_slot)).cloned() else {
                return false;
            };
            callee.ret = *ret;
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(receiver.clone());
            call_args.extend(args.iter().cloned());
            *stmt = Statement::Assign(
                place.clone(),
                Rvalue::Call {
                    callee,
                    args: call_args,
                },
            );
            true
        }
        _ => false,
    }
}
