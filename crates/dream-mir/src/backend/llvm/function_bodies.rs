//! Builds every function body. Contiguous chunks are built on separate threads, each on a fork of
//! the module state, and merged back in function order, so the module is the one a single
//! thread would build. A chunk that touched state numbered in build order is not merged; that
//! chunk and every later function are built here in order instead.

use super::body;
use super::construction;
use super::lcx::Lcx;
use dream_types::DefId;
use rayon::prelude::*;
use std::collections::BTreeSet;
use std::ops::Range;

/// Below this many functions per chunk, forking the module state costs more than it saves.
const MIN_CHUNK: usize = 256;

pub(super) fn build_all(l: &mut Lcx<'_>, units: Option<&[usize]>, fresh: &BTreeSet<DefId>) {
    let mir = l.mir;
    let mut async_index = Vec::with_capacity(mir.functions.len());
    let mut asyncs = 0;
    for f in &mir.functions {
        async_index.push(asyncs);
        asyncs += usize::from(f.is_async);
    }
    let count = mir.functions.len();
    let chunks = if l.cx.debug_syms {
        1
    } else {
        rayon::current_num_threads().min(count / MIN_CHUNK)
    };
    if chunks < 2 {
        for (index, &asyncs) in async_index.iter().enumerate() {
            build(l, index, asyncs, units, fresh);
        }
        return;
    }

    let base = l.sizes();
    let ranges: Vec<Range<usize>> = (0..chunks)
        .map(|chunk| chunk * count / chunks..(chunk + 1) * count / chunks)
        .collect();
    let shared = &*l;
    let forks: Vec<Lcx<'_>> = ranges
        .par_iter()
        .map(|range| {
            let mut fork = shared.fork();
            for index in range.clone() {
                build(&mut fork, index, async_index[index], units, fresh);
            }
            fork
        })
        .collect();
    let mut sequential_from = count;
    for (fork, range) in forks.into_iter().zip(&ranges) {
        if l.order_dependent(&fork, &base) {
            sequential_from = range.start;
            break;
        }
        l.merge(fork, &base);
    }
    for (index, &asyncs) in async_index.iter().enumerate().skip(sequential_from) {
        build(l, index, asyncs, units, fresh);
    }
}

fn build(
    l: &mut Lcx<'_>,
    index: usize,
    async_index: usize,
    units: Option<&[usize]>,
    fresh: &BTreeSet<DefId>,
) {
    let mir = l.mir;
    let f = &mir.functions[index];
    if let Some(units) = units {
        l.m.set_unit(units[index]);
    }
    if !f.is_async || f.hir_fn.is_none() {
        body::build_sync(l, f, construction::InitMode::Ordinary);
        if fresh.contains(&f.def) {
            body::build_sync(l, f, construction::InitMode::Fresh);
        }
        if f.is_async {
            body::build_empty_poll_drop(l, f);
        } else {
            body::build_abi_wrapper(l, f);
        }
        return;
    }
    let poll_idx = mir.functions.len() + 1 + async_index;
    let pre_lowered = &mir.polls[async_index];
    let (offs, frame_size) = body::async_offsets(l, pre_lowered);
    let environment =
        crate::async_emit::captures_environment(pre_lowered).then_some((frame_size + 7) & !7);
    let frame_size = environment.map_or(frame_size, |offset| offset + 8);
    body::build_async_stub(
        l,
        f,
        pre_lowered,
        &offs,
        frame_size,
        poll_idx as i32,
        environment,
    );
    body::build_poll(l, f, pre_lowered, &offs, environment);
    body::build_future_drop(l, f, pre_lowered, &offs, environment);
}
