use crate::Harness;
use std::time::Instant;

async fn value(i: i32) -> i32 {
    i.wrapping_add(1)
}

pub(super) async fn poll(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut total = 0i32;
    for i in 0..iters {
        let result = tokio::task::spawn_local(value(i as i32)).await.unwrap();
        total = total.wrapping_add(result);
    }
    h.report("async_poll", start, iters, total);
}

pub(super) async fn queue(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut total = 0i32;
    for i in 0..iters {
        let a = tokio::task::spawn_local(value(i as i32));
        let b = tokio::task::spawn_local(value(i as i32 + 1));
        let (a, b) = tokio::join!(a, b);
        total = total.wrapping_add(a.unwrap()).wrapping_add(b.unwrap());
    }
    h.report("scheduler_queue", start, iters, total);
}
