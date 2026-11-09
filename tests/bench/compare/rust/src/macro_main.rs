mod harness;
mod macro_formats;
mod macro_graph;
mod macro_tasks;

use harness::{Harness, env_bit};

struct Lcg(i32);
impl Lcg {
    fn next(&mut self, bound: i32) -> i32 {
        self.0 = self.0.wrapping_mul(1103515245).wrapping_add(12345);
        ((self.0 >> 8) & 16777215) % bound
    }
}

async fn run_suite(h: &mut Harness) {
    macro_formats::json(h, 2000);
    macro_formats::logs(h, 10);
    macro_graph::run(h, 25);
    macro_tasks::fanout(h, 200).await;
    macro_tasks::parallel(h, 50);
}

fn main() {
    let mut h = Harness {
        measuring: false,
        sink: 0,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    runtime.block_on(async {
        run_suite(&mut h).await;
        h.measuring = true;
        for _ in 0..harness::passes() {
            run_suite(&mut h).await;
        }
    });
    println!("sink {}", h.sink);
}
