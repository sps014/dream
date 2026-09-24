//! Static guarded devirtualization: an interface slot with at most [`MAX_GUARDED`] implementors
//! dispatches as a tag test into a direct call. Two arms are a ternary; three or four are a
//! `switch` on the tag so clang can emit a jump table and inline the callees. The direct arms
//! call through the same function-pointer type the itable uses, so the ABI matches the dynamic
//! path. The itable fallback stays for a tag that is not a registered implementor.

use super::ctx::Cx;
use super::protocol::interface_tag;
use super::types::c_ident;
use crate::backend::shared::func_symbol;
use indexmap::IndexMap;
use std::collections::HashMap;

/// Four is the largest closed set that still beats an itable call once the arms inline.
/// Past that, a mixed receiver is cheaper as an indirect call than as a long test chain.
const MAX_GUARDED: usize = 4;

/// `(runtime tag, C symbol)` arms in tag order.
type Arms = Vec<(i32, String)>;

/// `(iface, slot)` → its guard arms.
pub(super) type GuardTable = HashMap<(usize, usize), Arms>;

pub(super) fn build_guards(cx: &Cx<'_>) -> GuardTable {
    let by_name: IndexMap<&str, &crate::MirFunction> = cx
        .mir
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();
    let mut arms: IndexMap<(usize, usize), Option<Arms>> = IndexMap::new();
    for imp in &cx.mir.interfaces.impls {
        let tag = interface_tag(cx, imp.class_ty);
        for (iid, symbols) in &imp.entries {
            for (slot, sym) in symbols.iter().enumerate() {
                let slot_arms = arms.entry((*iid, slot)).or_insert_with(|| Some(Vec::new()));
                let resolved = tag.zip(by_name.get(sym.as_str()));
                let (Some(list), Some((tag, f))) = (slot_arms.as_mut(), resolved) else {
                    *slot_arms = None;
                    continue;
                };
                let cname = c_ident(&func_symbol(f));
                match list.iter().find(|(t, _)| *t == tag) {
                    Some((_, prev)) if *prev == cname => {}
                    Some(_) => *slot_arms = None,
                    None => list.push((tag, cname)),
                }
            }
        }
    }
    arms.into_iter()
        .filter_map(|(key, list)| {
            let mut list = list?;
            // `impls` order is not stable across compiles; tags are.
            list.sort();
            (!list.is_empty() && list.len() <= MAX_GUARDED).then_some((key, list))
        })
        .collect()
}
