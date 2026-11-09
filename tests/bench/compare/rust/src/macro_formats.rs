use crate::{Harness, Lcg, env_bit};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Instant};

#[derive(Serialize, Deserialize)]
struct Item {
    sku: String,
    qty: i32,
    price: f64,
}
#[derive(Serialize, Deserialize)]
struct Order {
    id: i32,
    customer: String,
    tags: Vec<String>,
    items: Vec<Item>,
}
#[derive(Serialize)]
struct Receipt {
    order: i32,
    customer: String,
    total: f64,
    units: i32,
    skus: Vec<String>,
}

fn order(rng: &mut Lcg, id: i32) -> String {
    let tags = vec![
        format!("priority-{}", rng.next(4)),
        format!("region-{}", rng.next(16)),
    ];
    let n = 8 + rng.next(24);
    let items = (0..n)
        .map(|_| Item {
            sku: format!("SKU-{}", rng.next(5000)),
            qty: 1 + rng.next(9),
            price: f64::from(rng.next(100000)) / 100.,
        })
        .collect();
    serde_json::to_string(&Order {
        id,
        customer: format!("customer-{}", rng.next(100000)),
        tags,
        items,
    })
    .unwrap()
}

pub(super) fn json(h: &mut Harness, iters: usize) {
    let mut rng = Lcg(17 + env_bit());
    let docs: Vec<_> = (0..64).map(|d| order(&mut rng, d)).collect();
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        let ord: Order = serde_json::from_str(&docs[i & 63]).unwrap();
        let (mut total, mut units, mut skus) = (0., 0i32, Vec::new());
        for it in ord.items {
            total += it.price * f64::from(it.qty);
            units += it.qty;
            if it.qty > 5 {
                skus.push(it.sku);
            }
        }
        let count = skus.len() as i32;
        let out = serde_json::to_vec(&Receipt {
            order: ord.id,
            customer: ord.customer,
            total,
            units,
            skus,
        })
        .unwrap();
        h.sink(out.len() as i32);
        acc = acc.wrapping_add(ord.id + units * 3 + count * 7 + ord.tags.len() as i32);
    }
    h.report("json_service", start.elapsed(), iters, acc);
}

fn log(rng: &mut Lcg) -> String {
    let levels = ["INFO", "WARN", "ERROR", "DEBUG"];
    let paths = [
        "/api/users",
        "/api/orders",
        "/static/app.js",
        "/health",
        "/api/search",
        "/login",
    ];
    let mut out = String::with_capacity(2000 * 64);
    for _ in 0..2000 {
        use std::fmt::Write;
        let minute = 10 + rng.next(50);
        let level = levels[rng.next(4) as usize];
        let path = paths[rng.next(6) as usize];
        let status = 200 + rng.next(4) * 100 + rng.next(3);
        let ms = rng.next(900);
        let user = rng.next(500);
        writeln!(
            out,
            "2026-10-09T12:{minute} {level} {path} status={status} ms={ms} user=u{user}"
        )
        .unwrap();
    }
    out
}

pub(super) fn logs(h: &mut Harness, iters: usize) {
    let text = log(&mut Lcg(29 + env_bit()));
    let status = regex::Regex::new(r"status=(5\d\d)").unwrap();
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        let mut paths = HashMap::new();
        let mut errors = 0;
        for line in text.split('\n').filter(|line| !line.is_empty()) {
            let parts: Vec<_> = line.split(' ').collect();
            let key = format!("{} {}", parts[1], parts[2]);
            let count = paths.get(&key).copied().unwrap_or(0) + 1;
            paths.insert(key, count);
            if status.is_match(line) {
                errors += 1;
            }
        }
        acc = acc.wrapping_add(paths.len() as i32 * 1000 + errors);
    }
    h.report("log_processor", start.elapsed(), iters, acc);
}
