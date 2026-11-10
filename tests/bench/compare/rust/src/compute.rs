use crate::{Harness, env_bit};
use std::time::Instant;

pub(super) fn nbody(h: &mut Harness, iters: usize) {
    struct Body {
        x: f64,
        y: f64,
        z: f64,
        vx: f64,
        vy: f64,
        vz: f64,
        mass: f64,
    }
    let mut bodies = vec![
        Body {
            x: 0.,
            y: 0.,
            z: 0.,
            vx: 0.01,
            vy: 0.,
            vz: 0.,
            mass: 1.,
        },
        Body {
            x: 1.,
            y: 0.5,
            z: 0.2,
            vx: 0.,
            vy: 0.02,
            vz: 0.,
            mass: 0.5,
        },
        Body {
            x: -1.2,
            y: 0.3,
            z: -0.4,
            vx: 0.01,
            vy: 0.,
            vz: 0.03,
            mass: 0.25,
        },
        Body {
            x: 0.4,
            y: -0.9,
            z: 0.8,
            vx: -0.02,
            vy: 0.01,
            vz: 0.,
            mass: 0.125,
        },
        Body {
            x: 0.9,
            y: 0.9,
            z: -0.7,
            vx: 0.,
            vy: -0.01,
            vz: 0.02,
            mass: 0.0625,
        },
    ];
    let start = Instant::now();
    for _ in 0..iters {
        for a in 0..bodies.len() {
            for b in a + 1..bodies.len() {
                let (left, right) = bodies.split_at_mut(b);
                let (a, b) = (&mut left[a], &mut right[0]);
                let (dx, dy, dz) = (b.x - a.x, b.y - a.y, b.z - a.z);
                let inv = 1. / (dx * dx + dy * dy + dz * dz + 1e-9).sqrt();
                let f = b.mass * a.mass * inv * inv * inv * 0.01;
                a.vx += dx * f;
                a.vy += dy * f;
                a.vz += dz * f;
                b.vx -= dx * f;
                b.vy -= dy * f;
                b.vz -= dz * f;
            }
        }
        for b in &mut bodies {
            b.x += b.vx * 0.01;
            b.y += b.vy * 0.01;
            b.z += b.vz * 0.01;
        }
    }
    h.report(
        "nbody",
        start.elapsed(),
        iters,
        (bodies[0].x * 1000.) as i32,
    );
}

pub(super) fn mandelbrot(h: &mut Harness, iters: usize) {
    let jitter = env_bit() as f64;
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        for row in 0..48 {
            let ci = row as f64 / 24. - 1.;
            for col in 0..64 {
                let cr = (col as f64 + jitter) / 32. - 1.5;
                let (mut zr, mut zi, mut k) = (0., 0., 0);
                while k < 30 {
                    let zr2 = zr * zr - zi * zi + cr;
                    zi = 2. * zr * zi + ci;
                    zr = zr2;
                    k += 1;
                    if zr * zr + zi * zi > 4. {
                        break;
                    }
                }
                acc = acc.wrapping_add(k);
            }
        }
    }
    h.report("mandelbrot", start.elapsed(), iters, acc);
}

pub(super) fn matmul(h: &mut Harness, iters: usize) {
    let a = (0..4096).map(|i| (i % 13) as f64).collect::<Vec<_>>();
    let b = (0..4096).map(|i| ((i * 7) % 11) as f64).collect::<Vec<_>>();
    let mut c = vec![0.; 4096];
    let start = Instant::now();
    for _ in 0..iters {
        for i in 0..64 {
            for k in 0..64 {
                let aik = a[i * 64 + k];
                for j in 0..64 {
                    c[i * 64 + j] += aik * b[k * 64 + j];
                }
            }
        }
    }

    h.report("matmul_64", start.elapsed(), iters, c[0] as i32);
}

fn qsort(a: &mut [i32], lo: isize, hi: isize) {
    if lo >= hi {
        return;
    }
    let p = a[((lo + hi) / 2) as usize];
    let (mut i, mut j) = (lo, hi);
    while i <= j {
        while a[i as usize] < p {
            i += 1;
        }
        while a[j as usize] > p {
            j -= 1;
        }
        if i <= j {
            a.swap(i as usize, j as usize);
            i += 1;
            j -= 1;
        }
    }
    qsort(a, lo, j);
    qsort(a, i, hi);
}

pub(super) fn quicksort(h: &mut Harness, iters: usize) {
    let mut a = vec![0i32; 512];
    let mut seed = 123456789i32;
    let start = Instant::now();
    for _ in 0..iters {
        for v in &mut a {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            *v = (seed >> 16) & 1023;
        }
        qsort(&mut a, 0, 511);
    }
    h.report("quicksort", start.elapsed(), iters, a[0]);
}

pub(super) fn sieve(h: &mut Harness, iters: usize) {
    let mut flags = vec![0i32; 4096];
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        flags.fill(1);
        for i in 2..64 {
            if flags[i] == 1 {
                for m in (i * i..4096).step_by(i) {
                    flags[m] = 0;
                }
            }
        }
        for &v in &flags[2..] {
            acc = acc.wrapping_add(v);
        }
    }
    h.report("sieve", start.elapsed(), iters, acc);
}

fn fib_value(n: i32) -> i32 {
    if n < 2 {
        n
    } else {
        fib_value(n - 1).wrapping_add(fib_value(n - 2))
    }
}

pub(super) fn fib(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        acc = acc.wrapping_add(fib_value(20 + (i & 1) as i32));
    }
    h.report("fib_rec", start.elapsed(), iters, acc);
}

trait Op {
    fn apply(&self, value: i32) -> i32;
}
struct Add;
struct Mul;
struct Sub;
struct Xor;
impl Op for Add {
    fn apply(&self, v: i32) -> i32 {
        v.wrapping_add(3)
    }
}
impl Op for Mul {
    fn apply(&self, v: i32) -> i32 {
        v.wrapping_mul(3)
    }
}
impl Op for Sub {
    fn apply(&self, v: i32) -> i32 {
        v.wrapping_sub(3)
    }
}
impl Op for Xor {
    fn apply(&self, v: i32) -> i32 {
        v ^ 3
    }
}

#[inline(never)]
fn iface_rounds(ops: &[&dyn Op], rounds: usize) -> i32 {
    let mut acc = 0i32;
    for r in 0..rounds {
        for op in ops {
            acc = acc.wrapping_add(op.apply(r as i32));
        }
    }
    acc
}

pub(super) fn interface(h: &mut Harness, iters: usize) {
    let choices: [&dyn Op; 4] = [&Add, &Mul, &Sub, &Xor];
    let mut seed = 12345i32;
    let ops: Vec<_> = (0..1024)
        .map(|_| {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            choices[((seed >> 16) & 3) as usize]
        })
        .collect();
    let rounds = (iters * 3 / 1024).max(1);
    let start = Instant::now();
    let acc = iface_rounds(&ops, rounds);
    h.report("iface_dispatch", start.elapsed(), rounds * 1024, acc);
}

#[inline(never)]
fn closure_offset(offset: i32) -> Box<dyn Fn(i32) -> i32> {
    Box::new(move |v| v.wrapping_add(offset))
}

#[inline(never)]
fn closure_scale(factor: i32) -> Box<dyn Fn(i32) -> i32> {
    Box::new(move |v| v.wrapping_mul(factor))
}

#[inline(never)]
fn closure_calls(callback: &dyn Fn(i32) -> i32, iters: usize) -> i32 {
    (0..iters).fold(0i32, |sum, i| sum.wrapping_add(callback(i as i32)))
}

pub(super) fn closure(h: &mut Harness, iters: usize) {
    let seed = env_bit();
    let callback = if seed == 1 {
        closure_offset(seed)
    } else {
        closure_scale(seed + 2)
    };
    let start = Instant::now();
    let acc = closure_calls(callback.as_ref(), iters);
    h.report("closure_dispatch", start.elapsed(), iters, acc);
}

pub(super) fn options(h: &mut Harness, iters: usize) {
    let seed = env_bit();
    let opts: Vec<_> = (0..8)
        .map(|k| (((k + seed) & 1) == 0).then_some(k + 7))
        .collect();
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        acc = acc.wrapping_add(opts[i & 7].unwrap_or(-1));
        acc = acc.wrapping_add(opts[(i + 3) & 7].unwrap_or(1));
    }
    h.report("sum_options", start.elapsed(), iters, acc);
}

pub(super) fn arrays(h: &mut Harness, iters: usize, integers: bool) {
    let mut a = (0..256).map(|k| k as f32).collect::<Vec<_>>();
    let b = (0..256).map(|k| (k + 1) as f32).collect::<Vec<_>>();
    let mut ai = (0..256).collect::<Vec<i32>>();
    let bi = (1..257).collect::<Vec<i32>>();
    let start = Instant::now();
    for _ in 0..iters {
        for (a, b) in a.iter_mut().zip(&b) {
            *a += b;
        }
        if integers {
            for (a, b) in ai.iter_mut().zip(&bi) {
                *a += b;
            }
        }
    }
    let elapsed = start.elapsed();
    let acc = (0..256).fold(0i32, |sum, i| {
        sum.wrapping_add(a[i] as i32)
            .wrapping_add(if integers { ai[i] } else { 0 })
    });
    h.report(
        if integers { "arr_add" } else { "vec_add" },
        elapsed,
        iters,
        acc,
    );
}
