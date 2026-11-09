use crate::Harness;
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

pub(super) fn list_push(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut list = Vec::with_capacity(iters);
    for i in 0..iters {
        list.push(i as i32);
    }
    h.report("list_push", start.elapsed(), iters, list.len() as i32);
}

pub(super) fn insert_mid(h: &mut Harness, iters: usize) {
    let rounds = (iters / 256).max(1);
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..rounds {
        let mut list = Vec::with_capacity(256);
        for i in 0..256 {
            list.insert(list.len() / 2, i);
        }
        acc = acc.wrapping_add(list[list.len() / 2]);
    }
    h.report("list_insert_mid", start.elapsed(), rounds * 256, acc);
}

pub(super) fn map(h: &mut Harness, iters: usize) {
    let mut map = HashMap::with_capacity(iters);
    let start = Instant::now();
    for i in 0..iters {
        map.insert(i as i32, (i as i32).wrapping_mul(3));
    }
    let mut acc = 0i32;
    for i in 0..iters {
        acc = acc.wrapping_add(map[&(i as i32)]);
    }
    h.report("map_get_set", start.elapsed(), iters, acc);
}

pub(super) fn map_clear(h: &mut Harness, iters: usize) {
    let mut map = HashMap::with_capacity(1024);
    let per = (iters / 64).max(1);
    let start = Instant::now();
    for _ in 0..64 {
        for i in 0..per {
            map.insert(i as i32, i as i32);
        }
        map.clear();
    }
    h.report(
        "map_clear_reuse",
        start.elapsed(),
        64 * per,
        map.len() as i32,
    );
}

pub(super) fn set(h: &mut Harness, iters: usize) {
    let mut set = HashSet::new();
    let start = Instant::now();
    for i in 0..iters {
        set.insert(i as i32);
    }
    let mut acc = 0i32;
    for i in 0..iters {
        if set.contains(&(i as i32)) {
            acc += 1;
        }
        if set.remove(&(i as i32)) {
            acc += 1;
        }
    }
    h.report("set_probe", start.elapsed(), iters, acc + set.len() as i32);
}

pub(super) fn set_clear(h: &mut Harness, iters: usize) {
    let mut set = HashSet::with_capacity(1024);
    let per = (iters / 64).max(1);
    let start = Instant::now();
    for _ in 0..64 {
        for i in 0..per {
            set.insert(i as i32);
        }
        set.clear();
    }
    h.report(
        "set_clear_reuse",
        start.elapsed(),
        64 * per,
        set.len() as i32,
    );
}

pub(super) fn list_clear(h: &mut Harness, iters: usize) {
    let mut list = Vec::with_capacity(64);
    let per = (iters / 64).max(1);
    let start = Instant::now();
    for _ in 0..64 {
        for i in 0..per {
            list.push(i as i32);
        }

        list.clear();
    }
    h.report(
        "list_clear_reuse",
        start.elapsed(),
        64 * per,
        list.len() as i32,
    );
}
