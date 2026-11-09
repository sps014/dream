use crate::{Harness, env_bit};
use std::{collections::HashMap, time::Instant};

pub(super) fn wordcount(h: &mut Harness, iters: usize) {
    let words: Vec<_> = "the quick brown fox jumps over the lazy dog the end the fox and the dog run with the quick fox".split(' ').collect();
    let mut map = HashMap::with_capacity(256);
    let start = Instant::now();
    for _ in 0..iters {
        for &w in &words {
            let n = map.get(w).copied().unwrap_or(0i32);
            map.insert(w, n.wrapping_add(1));
        }
    }
    h.report(
        "wordcount",
        start.elapsed(),
        iters * words.len(),
        map["the"],
    );
}

pub(super) fn parse(h: &mut Harness, iters: usize) {
    let seed = env_bit();
    let sources = (0..8)
        .map(|k| (1_000_000_000 + k * 111111 + seed).to_string())
        .collect::<Vec<_>>();
    let start = Instant::now();
    let mut acc = 0i64;
    for i in 0..iters {
        let mut v = 0i32;
        for &digit in sources[i & 7].as_bytes() {
            v = v * 10 + i32::from(digit) - 48;
        }
        acc = acc.wrapping_add(i64::from(v));
    }
    h.report("parse_ints", start.elapsed(), iters, acc as i32);
}

pub(super) fn concat(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        acc = acc.wrapping_add(format!("hello{i}world").len() as i32);
    }
    h.report("string_concat", start.elapsed(), iters, acc);
}

pub(super) fn equality(h: &mut Harness, iters: usize) {
    let base = "abcdefghijklmnopqrstuvwxyz0123456789";
    let variants = [
        "abcdefghijklmnopqrstuvwxyz0123456789",
        "abcdefghijklmnopqrstuvwxyz0123456780",
        "abcdefghijKlmnopqrstuvwxyz0123456789",
        "abcdefghijklmnopqrstuvwxyz012345678",
    ];
    let start = Instant::now();
    let mut hits = 0i32;
    for i in 0..iters {
        hits += if base == variants[i & 3] { 1 } else { -1 };
        h.sink(hits);
    }
    h.report("string_eq", start.elapsed(), iters, hits);
}

pub(super) fn scan(h: &mut Harness, iters: usize, bytes: bool) {
    let text = "The quick brown fox jumps over the lazy dog. 0123456789".to_owned();
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        if bytes {
            // Hide the bytes once per pass. A visible constant reduction becomes `iters * sum`.
            for &v in std::hint::black_box(text.as_bytes()) {
                acc = acc.wrapping_add(i32::from(v));
            }
        } else {
            for v in text.chars() {
                acc = acc.wrapping_add(v as i32);
            }
        }
    }
    h.report(
        if bytes { "byte_scan" } else { "char_scan" },
        start.elapsed(),
        iters,
        acc,
    );
}

pub(super) fn substring(h: &mut Harness, iters: usize) {
    struct Slice {
        owner: std::rc::Rc<str>,
        start: usize,
        end: usize,
    }
    let text: std::rc::Rc<str> = std::rc::Rc::from(format!(
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789{}",
        env_bit()
    ));
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        let offset = i & 7;
        let sub = Slice {
            owner: text.clone(),
            start: offset,
            end: offset + 25 + (i & 3),
        };
        acc = acc.wrapping_add(
            (sub.end - sub.start) as i32 + i32::from(sub.owner.as_bytes()[sub.start]),
        );
    }
    h.report("substring", start.elapsed(), iters, acc);
}

pub(super) fn span(h: &mut Harness, iters: usize) {
    let text = format!(
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789{}",
        env_bit()
    );
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        let st = i & 7;
        let sub = &text[st..st + 25 + (i & 3)];
        acc = acc.wrapping_add(sub.len() as i32 + i32::from(sub.as_bytes()[0]));
    }
    h.report("substring_span", start.elapsed(), iters, acc);
}

pub(super) fn split(h: &mut Harness, iters: usize) {
    let line = "alpha,beta,gamma,delta,epsilon,zeta,eta,theta";
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        for part in line.split(',') {
            acc = acc.wrapping_add(part.len() as i32);
        }
    }
    h.report("split_span", start.elapsed(), iters, acc);
}

pub(super) fn map_span(h: &mut Harness, iters: usize) {
    let text = "k0k1k2k3k4k5k6k7";
    let map: HashMap<_, _> = (0..8).map(|j| (format!("k{j}"), j + 1)).collect();
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        let k = (i & 7) * 2;
        acc = acc.wrapping_add(map[&text[k..k + 2]]);
    }
    h.report("map_get_span", start.elapsed(), iters, acc);
}

pub(super) fn builder(h: &mut Harness, iters: usize) {
    let chunk = "abcdefghijklmnopqrstuvwxyz";
    let mut text = String::with_capacity(iters * chunk.len());
    let start = Instant::now();
    for _ in 0..iters {
        text.push_str(chunk);
    }
    h.report("string_builder", start.elapsed(), iters, text.len() as i32);
}
