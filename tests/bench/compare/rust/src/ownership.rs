use crate::Harness;
use std::{
    hint::black_box,
    rc::{Rc, Weak},
    time::Instant,
};

struct Tree {
    _left: Option<Rc<Tree>>,
    _right: Option<Rc<Tree>>,
}

fn tree(depth: usize) -> Option<Rc<Tree>> {
    (depth > 0).then(|| {
        Rc::new(Tree {
            _left: tree(depth - 1),
            _right: tree(depth - 1),
        })
    })
}

pub(super) fn trees_alloc(h: &mut Harness, iters: usize) {
    let mut keep = Vec::with_capacity(iters);
    let start = Instant::now();
    for _ in 0..iters {
        keep.push(black_box(tree(12)));
    }
    h.report("binary_trees_alloc", start, iters, keep.len() as i32);
    black_box(&keep);
}

pub(super) fn trees(h: &mut Harness, iters: usize, reclaim: bool) {
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        let root = black_box(tree(12));
        acc += if root.is_some() { 1 } else { -1 };
        drop(root);
    }
    h.report(
        if reclaim {
            "binary_trees_reclaim"
        } else {
            "binary_trees"
        },
        start,
        iters,
        acc,
    );
}

struct Link {
    value: i32,
    next: Option<Rc<Link>>,
}

pub(super) fn linked(h: &mut Harness, iters: usize) {
    let mut head = None;
    for i in 0..1024 {
        head = Some(Rc::new(Link {
            value: i,
            next: head,
        }));
    }
    let head = black_box(head);
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        let mut current = head.clone();
        while let Some(node) = current {
            acc = acc.wrapping_add(node.value);
            current = node.next.clone();
        }
    }
    h.report("linked_walk", start, iters, acc);
}

struct WeakTree {
    left: Option<Rc<WeakTree>>,
    _right: Option<Rc<WeakTree>>,
    _parent: Weak<WeakTree>,
}

fn weak_tree(depth: usize, parent: Weak<WeakTree>) -> Rc<WeakTree> {
    Rc::new_cyclic(|this| WeakTree {
        left: (depth > 1).then(|| weak_tree(depth - 1, this.clone())),
        _right: (depth > 1).then(|| weak_tree(depth - 1, this.clone())),
        _parent: parent,
    })
}

pub(super) fn weak(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut acc = 0i32;
    for _ in 0..iters {
        let root = black_box(weak_tree(10, Weak::new()));
        acc += if root.left.is_some() { 1 } else { -1 };
    }
    h.report("weak_tree", start, iters, acc);
}

pub(super) fn arc(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        let a: Rc<str> = Rc::from(format!("n{i}"));
        let b = a.clone();
        let c = b.clone();
        acc = acc.wrapping_add(black_box(&c).len() as i32);
    }
    h.report("arc_locals", start, iters, acc);
}

#[inline(never)]
fn touch(buf: &[i32]) -> i32 {
    buf[0]
}

pub(super) fn alloc(h: &mut Harness, iters: usize) {
    let start = Instant::now();
    let mut acc = 0i32;
    for i in 0..iters {
        let mut buf = vec![0i32; 64];
        buf[0] = i as i32;
        acc = acc.wrapping_add(touch(black_box(&buf)));
    }
    h.report("alloc_churn", start, iters, acc);
}
