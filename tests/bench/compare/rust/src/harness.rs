use std::time::Duration;

pub(super) struct Harness {
    pub measuring: bool,
    pub sink: i32,
}

impl Harness {
    #[inline(never)]
    pub fn sink(&mut self, value: i32) {
        self.sink = self.sink.wrapping_add(value);
    }

    pub fn report(&mut self, name: &str, elapsed: Duration, iters: usize, checksum: i32) {
        let nanos = elapsed.as_nanos();
        self.sink(checksum);
        if self.measuring {
            println!(
                "bench {name} ns_total={nanos} iters={iters} ns_per_op={} checksum={checksum}",
                nanos / iters as u128
            );
        }
    }
}

pub(super) fn env_bit() -> i32 {
    std::env::var("DREAM_BENCH_SEED")
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(1)
        & 1
}

pub(super) fn passes() -> usize {
    std::env::var("DREAM_BENCH_PASSES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, 30)
}
