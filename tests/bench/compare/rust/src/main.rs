//! Rust counterpart of microbenches.dream and micro.go, including their 32-bit wrapping results.

mod collections;
mod compute;
mod formats;
mod ownership;
mod scheduler;
mod strings;

mod harness;
use harness::{Harness, env_bit};

fn run_suite(h: &mut Harness, s: usize) {
    compute::nbody(h, s * 4);
    compute::mandelbrot(h, s / 100);
    compute::matmul(h, s / 50);
    compute::quicksort(h, s / 20);
    compute::sieve(h, s / 12);
    compute::fib(h, s / 20);
    compute::interface(h, s * 12);
    compute::closure(h, s * 12);
    ownership::trees(h, s / 200, false);
    ownership::trees_alloc(h, s / 200);
    ownership::linked(h, s / 5);
    ownership::weak(h, s / 200);
    strings::wordcount(h, s);
    strings::parse(h, s * 50);
    compute::options(h, s * 400);
    ownership::arc(h, s * 25);
    strings::concat(h, s * 20);
    strings::equality(h, s * 100);
    strings::scan(h, s * 10, false);
    strings::scan(h, s * 20, true);
    strings::substring(h, s * 160);
    strings::span(h, s * 160);
    strings::split(h, s * 20);
    collections::list_push(h, s * 400);
    collections::insert_mid(h, s * 40);
    collections::map(h, s * 35);
    strings::map_span(h, s * 35);
    collections::map_clear(h, s * 70);
    collections::set(h, s * 35);
    collections::set_clear(h, s * 70);
    collections::list_clear(h, s * 500);
    ownership::alloc(h, s * 30);
    formats::regex(h, s / 2);
    strings::builder(h, s * 20);
    formats::serialize(h, s * 2);
    formats::deserialize(h, s / 2);
    compute::arrays(h, s * 5, true);
    compute::arrays(h, s * 10, false);
    ownership::trees(h, s / 200, true);
}

fn main() {
    let mut h = Harness {
        measuring: false,
        sink: 0,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let tasks = tokio::task::LocalSet::new();
    tasks.block_on(&runtime, async {
        run_suite(&mut h, 20_000);
        scheduler::poll(&mut h, 20_000).await;
        scheduler::queue(&mut h, 5_000).await;
        h.measuring = true;
        for _ in 0..harness::passes() {
            run_suite(&mut h, 20_000);
            scheduler::poll(&mut h, 20_000).await;
            scheduler::queue(&mut h, 5_000).await;
        }
    });
    println!("sink {}", h.sink);
}
