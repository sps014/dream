//! Disjoint-set forest over the dense indices `0..n`.

/// Every set's root is its smallest member, so roots do not depend on merge order and callers
/// may key deterministic output by them.
pub(crate) struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    pub(crate) fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }

    pub(crate) fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    pub(crate) fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra.max(rb)] = ra.min(rb);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UnionFind;

    #[test]
    fn root_is_smallest_member_in_any_merge_order() {
        let mut uf = UnionFind::new(6);
        uf.union(5, 3);
        uf.union(4, 5);
        uf.union(3, 1);
        assert!([1, 3, 4, 5].iter().all(|&x| uf.find(x) == 1));
        assert_eq!(uf.find(0), 0);
        assert_eq!(uf.find(2), 2);
    }
}
