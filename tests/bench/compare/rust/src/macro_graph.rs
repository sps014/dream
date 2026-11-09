use crate::{Harness, Lcg, env_bit};
use std::{rc::Rc, time::Instant};

struct Edge {
    to: i32,
    weight: i32,
}
struct Vertex {
    _id: i32,
    out: Vec<Edge>,
}
struct HeapVertex {
    _id: i32,
    out: Vec<Rc<Edge>>,
}
struct Values(Vec<Vertex>);
struct Objects(Vec<Rc<HeapVertex>>);

trait Graph {
    fn len(&self) -> usize;
    fn edges(&self, node: usize) -> impl Iterator<Item = (i32, i32)>;
}
impl Graph for Values {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn edges(&self, node: usize) -> impl Iterator<Item = (i32, i32)> {
        self.0[node].out.iter().map(|e| (e.to, e.weight))
    }
}
impl Graph for Objects {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn edges(&self, node: usize) -> impl Iterator<Item = (i32, i32)> {
        self.0[node].out.iter().map(|e| (e.to, e.weight))
    }
}

fn values() -> Values {
    let mut rng = Lcg(41 + env_bit());
    let mut g: Vec<_> = (0..2000)
        .map(|id| Vertex {
            _id: id,
            out: Vec::new(),
        })
        .collect();
    for i in 0..2000 {
        for _ in 0..6 {
            g[i].out.push(Edge {
                to: rng.next(2000),
                weight: 1 + rng.next(100),
            });
        }
        g[(i + 1) % 2000].out.push(Edge {
            to: i as i32,
            weight: 50,
        });
    }
    Values(g)
}

fn objects() -> Objects {
    let Values(g) = values();
    Objects(
        g.into_iter()
            .map(|v| {
                Rc::new(HeapVertex {
                    _id: v._id,
                    out: v.out.into_iter().map(Rc::new).collect(),
                })
            })
            .collect(),
    )
}

struct Visit {
    node: i32,
    dist: i32,
}
trait Entry {
    fn new(node: i32, dist: i32) -> Self;
    fn node(&self) -> i32;
    fn dist(&self) -> i32;
}
impl Entry for Visit {
    fn new(node: i32, dist: i32) -> Self {
        Self { node, dist }
    }
    fn node(&self) -> i32 {
        self.node
    }
    fn dist(&self) -> i32 {
        self.dist
    }
}
impl Entry for Rc<Visit> {
    fn new(node: i32, dist: i32) -> Self {
        Rc::new(Visit { node, dist })
    }
    fn node(&self) -> i32 {
        self.node
    }
    fn dist(&self) -> i32 {
        self.dist
    }
}

// Dream and Go use four children per heap node; BinaryHeap would change the algorithm.
struct Heap4<T>(Vec<T>);
impl<T: Entry> Heap4<T> {
    fn push(&mut self, value: T) {
        self.0.push(value);
        let mut i = self.0.len() - 1;
        while i > 0 {
            let parent = (i - 1) / 4;
            if self.0[i].dist() >= self.0[parent].dist() {
                break;
            }
            self.0.swap(i, parent);
            i = parent;
        }
    }

    fn pop(&mut self) -> Option<T> {
        let last = self.0.pop()?;
        if self.0.is_empty() {
            return Some(last);
        }
        let top = std::mem::replace(&mut self.0[0], last);
        let mut i = 0;
        loop {
            let first = i * 4 + 1;
            if first >= self.0.len() {
                break;
            }
            let mut best = first;
            for j in first + 1..(first + 4).min(self.0.len()) {
                if self.0[j].dist() < self.0[best].dist() {
                    best = j;
                }
            }
            if self.0[best].dist() >= self.0[i].dist() {
                break;
            }
            self.0.swap(i, best);
            i = best;
        }
        Some(top)
    }
}

fn shortest<G: Graph, T: Entry>(g: &G, source: usize) -> i32 {
    let mut dist = vec![i32::MAX; g.len()];
    let mut queue = Heap4::<T>(Vec::new());
    dist[source] = 0;
    queue.push(T::new(source as i32, 0));
    while let Some(v) = queue.pop() {
        if v.dist() > dist[v.node() as usize] {
            continue;
        }
        for (to, weight) in g.edges(v.node() as usize) {
            let next = v.dist() + weight;
            if next < dist[to as usize] {
                dist[to as usize] = next;
                queue.push(T::new(to, next));
            }
        }
    }
    dist.into_iter()
        .filter(|&d| d != i32::MAX)
        .fold(0i32, i32::wrapping_add)
}

fn measure<G: Graph, T: Entry>(h: &mut Harness, name: &str, g: &G, iters: usize) {
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        acc = acc.wrapping_add(shortest::<G, T>(g, (i * 37) % 2000));
    }
    h.report(name, start.elapsed(), iters, acc);
}

pub(super) fn run(h: &mut Harness, iters: usize) {
    let g = values();
    measure::<_, Rc<Visit>>(h, "graph_paths_class", &g, iters);
    measure::<_, Visit>(h, "graph_paths_value", &g, iters);
    let g = objects();
    measure::<_, Rc<Visit>>(h, "graph_paths_heap", &g, iters);
}
