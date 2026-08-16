//! Deterministic function symbols and runtime type tags shared by every backend.

use super::{Mir, MirFunction};
use dream_types::{TypeId, DefId};
use std::collections::HashMap;

const STRUCT_TAG_BASE: i32 = super::abi::TAG_STRUCT_BASE;

pub fn func_symbol(func: &MirFunction) -> String {
    if func.instance.is_empty() {
        func.name.clone()
    } else {
        let args: Vec<String> = func.instance.iter().map(|t| t.0.to_string()).collect();
        format!("{}__{}", func.name, args.join("_"))
    }
}

pub fn poll_symbol(func: &MirFunction) -> String {
    format!("poll_{}", func_symbol(func))
}

pub fn struct_tags(mir: &Mir) -> HashMap<TypeId, i32> {
    mir.layouts
        .structs
        .keys()
        .chain(mir.layouts.unions.keys())
        .enumerate()
        .map(|(i, ty)| (*ty, STRUCT_TAG_BASE + i as i32))
        .collect()
}

pub fn symbol_table(mir: &Mir) -> HashMap<(DefId, Vec<TypeId>), String> {
    let mut table: HashMap<(DefId, Vec<TypeId>), String> = mir
        .functions
        .iter()
        .map(|f| ((f.def, f.instance.clone()), func_symbol(f)))
        .collect();
    for imp in &mir.imports {
        table.insert((imp.def, vec![]), imp.name.clone());
    }
    for (def, key) in &mir.intrinsics {
        table.entry((*def, vec![])).or_insert_with(|| key.clone());
    }
    table
}
