//! Inline values have separate storage, except references rooted outside the function.

use crate::{Const, MirFunction, Operand, Place, Rvalue, Statement};
use indexmap::IndexSet;

pub(super) fn external_value_references(f: &MirFunction) -> IndexSet<u32> {
    let mut external: IndexSet<u32> = f
        .params
        .iter()
        .filter(|local| f.locals[local.0 as usize].is_ref)
        .map(|local| local.0)
        .collect();
    let mut edges = Vec::new();
    for stmt in f.blocks.iter().flat_map(|block| &block.stmts) {
        let Statement::Assign(Place::Local(dest), rv) = stmt else {
            continue;
        };
        if !f.locals[dest.0 as usize].is_ref {
            continue;
        }
        match rv {
            Rvalue::Use(Operand::Copy(Place::Local(src))) | Rvalue::Move { src, .. } => {
                edges.push((dest.0, src.0))
            }
            Rvalue::Use(Operand::Const(Const::Null)) | Rvalue::New { .. } | Rvalue::Zero(_) => {}
            _ => {
                external.insert(dest.0);
            }
        }
    }
    loop {
        let before = external.len();
        for &(dest, src) in &edges {
            if external.contains(&src) {
                external.insert(dest);
            }
        }
        if external.len() == before {
            break;
        }
    }
    external
}
