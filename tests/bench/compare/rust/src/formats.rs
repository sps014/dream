use crate::Harness;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Serialize, Deserialize)]
struct Address {
    city: String,
    zip: String,
}

#[derive(Serialize, Deserialize)]
struct User {
    name: String,
    age: i32,
    active: bool,
    address: Address,
    tags: Vec<String>,
    scores: Vec<i32>,
}

fn user() -> User {
    User {
        name: "Ada".into(),
        age: 36,
        active: true,
        address: Address {
            city: "London".into(),
            zip: "NW1".into(),
        },
        tags: vec!["dev".into(), "math".into()],
        scores: vec![10, 20, 30],
    }
}

pub(super) fn serialize(h: &mut Harness, iters: usize) {
    let u = user();
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        acc = acc.wrapping_add(serde_json::to_vec(&u).unwrap().len() as i32);
    }
    h.report("json_serialize", start.elapsed(), iters, acc);
}

pub(super) fn deserialize(h: &mut Harness, iters: usize) {
    let text = serde_json::to_vec(&user()).unwrap();
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        let back: User = serde_json::from_slice(&text).unwrap_or_else(|_| user());
        acc = acc.wrapping_add(back.age + back.scores[2]);
    }
    h.report("json_deserialize", start.elapsed(), iters, acc);
}

pub(super) fn regex(h: &mut Harness, iters: usize) {
    let re = regex::Regex::new(r"[a-z]+\d+").unwrap();
    let text = "abc123def456ghi789xyz";
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        let matches = re.find_iter(text).collect::<Vec<_>>();
        for found in matches {
            acc = acc.wrapping_add(found.len() as i32);
        }
    }
    h.report("regex_find", start.elapsed(), iters, acc);
}
