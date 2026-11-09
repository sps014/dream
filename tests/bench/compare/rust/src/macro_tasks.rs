use crate::{Harness, env_bit};
use futures_util::future::{join_all, select_all};
use rayon::prelude::*;
use std::time::Instant;

async fn leaf(i: i32) -> i32 {
    if i & 7 == 0 {
        tokio::task::yield_now().await;
    }
    i.wrapping_mul(31) & 1023
}

async fn middle(base: i32) -> i32 {
    join_all((0..4).map(|p| leaf(base + p)))
        .await
        .into_iter()
        .fold(0i32, i32::wrapping_add)
}

pub(super) async fn fanout(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        let results = join_all((0..64).map(|k| middle((i + k * 4) as i32))).await;
        for v in results {
            acc = acc.wrapping_add(v);
        }
        let (first, _, remaining) =
            select_all([Box::pin(leaf(i as i32)), Box::pin(leaf(i as i32 + 1024))]).await;
        acc = acc.wrapping_add(first);
        join_all(remaining).await;
    }
    h.report("async_fanout", start.elapsed(), iters, acc);
}

fn chunk(mut seed: i32) -> i32 {
    let mut acc = 0i32;
    for _ in 0..20000 {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        acc = acc.wrapping_add((seed >> 16) & 255);
    }
    acc
}

pub(super) fn parallel(h: &mut Harness, iters: usize) {
    let bit = env_bit();
    let mut seeds = vec![0i32; 16];
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        for (k, seed) in seeds.iter_mut().enumerate() {
            *seed = (i * 16 + k) as i32 + bit;
        }
        let sum = seeds
            .par_iter()
            .map(|&seed| chunk(seed))
            .reduce(|| 0, i32::wrapping_add);
        acc = acc.wrapping_add(sum);
    }
    h.report("task_parallel", start.elapsed(), iters, acc);
}
