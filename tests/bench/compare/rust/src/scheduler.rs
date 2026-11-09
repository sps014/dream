use crate::Harness;
use std::time::Instant;

#[inline(never)]
async fn value(i: i32) -> i32 {
    i.wrapping_add(1)
}

pub(super) async fn poll(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut total = 0i32;
    for i in 0..iters {
        let result = value(i as i32).await;
        total = total.wrapping_add(result);
    }
    h.report("async_poll", start.elapsed(), iters, total);
}

pub(super) async fn queue(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut total = 0i32;
    for i in 0..iters {
        let values = futures_util::future::join_all([value(i as i32), value(i as i32 + 1)]).await;
        total = total.wrapping_add(values[0]).wrapping_add(values[1]);
    }
    h.report("scheduler_queue", start.elapsed(), iters, total);
}
