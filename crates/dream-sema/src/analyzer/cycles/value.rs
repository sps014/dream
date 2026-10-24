//! Abstract heap values: which objects an expression may denote, as a small sorted atom set.

use dream_types::TypeId;
use indexmap::IndexMap;
use smallvec::SmallVec;

/// One storage location kind. Stores invalidate access paths through the same kind.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(super) enum Slot {
    Field(TypeId, u32),
    Elem(TypeId),
    /// Union payloads are immutable once constructed.
    Payload(TypeId, u32, u32),
    /// A `weak` or `unowned` field. It owns nothing, so no cycle runs through it, but reading it
    /// yields what was stored, which may be anything, including an ancestor of the holder.
    Weak(TypeId, u32),
}

impl Slot {
    pub(super) fn owns(self) -> bool {
        !matches!(self, Slot::Weak(..))
    }
}

/// Where an async call's future of type `future` keeps the arguments its coroutine frame holds.
/// A future has no declared fields, so these indices name nothing else.
pub(super) fn frame_slot(future: TypeId) -> Slot {
    Slot::Field(future, u32::MAX - 1)
}

/// Where an async call's future of type `future` keeps the value awaiting it yields.
pub(super) fn result_slot(future: TypeId) -> Slot {
    Slot::Field(future, u32::MAX)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(super) struct PathId(u32);

impl PathId {
    pub(super) const ROOT: PathId = PathId(0);
    /// Anything reachable from the parameter, through owning or weak references, including the
    /// parameter itself.
    pub(super) const ANY: PathId = PathId(u32::MAX);
    /// An object that was strictly beneath the parameter until a store cut the path to it. It
    /// still cannot reach the parameter: any store whose value could reach the parameter turns
    /// these back into [`PathId::BENEATH`].
    pub(super) const DETACHED: PathId = PathId(u32::MAX - 1);
    /// An object other than the parameter's own that was beneath it during the call, which may
    /// since have been linked to anything, including the parameter.
    pub(super) const BENEATH: PathId = PathId(u32::MAX - 2);

    const LINKED: u32 = 1 << 29;
    const CUT: u32 = 1 << 30;

    /// A concrete access path rather than a summary of many.
    pub(super) fn is_exact(self) -> bool {
        self.0 < PathId::LINKED
    }

    /// The object that was at this exact path on entry, after a store cut a path to it: it is
    /// [`PathId::DETACHED`] but callers can still tell which of their objects it is.
    pub(super) fn cut(self) -> PathId {
        debug_assert!(self.is_exact());
        PathId(self.0 | PathId::CUT)
    }

    /// The object that was at this exact path on entry, which is [`PathId::BENEATH`] now.
    pub(super) fn linked(self) -> PathId {
        PathId(self.entry().0 | PathId::LINKED)
    }

    pub(super) fn is_cut(self) -> bool {
        self.0 >> 29 == 0b010
    }

    pub(super) fn is_linked(self) -> bool {
        self.0 >> 29 == 0b001
    }

    /// The entry path of a [`PathId::cut`] or [`PathId::linked`] path.
    pub(super) fn entry(self) -> PathId {
        PathId(self.0 & (PathId::LINKED - 1))
    }

    /// Detached from the parameter, whether or not the entry path is known.
    pub(super) fn is_detached(self) -> bool {
        self == PathId::DETACHED || self.is_cut()
    }

    /// Beneath the parameter but possibly linked to anything, whether or not the entry path
    /// is known.
    pub(super) fn is_beneath(self) -> bool {
        self == PathId::BENEATH || self.is_linked()
    }
}

const MAX_PATH: u8 = 8;

struct PathNode {
    parent: PathId,
    slot: Option<Slot>,
    len: u8,
    has_elem: bool,
    has_weak: bool,
}

/// Interned access paths shared by every function so summaries can carry them across calls.
pub(super) struct Paths {
    nodes: Vec<PathNode>,
    children: IndexMap<(PathId, Slot), PathId>,
}

impl Paths {
    pub(super) fn new() -> Self {
        Self {
            nodes: vec![PathNode {
                parent: PathId::ROOT,
                slot: None,
                len: 0,
                has_elem: false,
                has_weak: false,
            }],
            children: IndexMap::new(),
        }
    }

    pub(super) fn extend(&mut self, path: PathId, slot: Slot) -> PathId {
        if (path.is_cut() || path.is_linked()) && slot.owns() {
            let inner = self.extend(path.entry(), slot);
            return match inner {
                PathId::DETACHED if path.is_linked() => PathId::BENEATH,
                inner if !inner.is_exact() => inner,
                inner if path.is_cut() => inner.cut(),
                inner => inner.linked(),
            };
        }
        if !path.is_exact() {
            // A weak reference may lead back above the parameter, which detached objects
            // cannot reach.
            return if slot.owns() && path == PathId::DETACHED {
                path
            } else {
                PathId::ANY
            };
        }
        let node = &self.nodes[path.0 as usize];
        if node.len >= MAX_PATH {
            return if slot.owns() && !node.has_weak {
                PathId::DETACHED
            } else {
                PathId::ANY
            };
        }
        if let Some(&child) = self.children.get(&(path, slot)) {
            return child;
        }
        let child = PathId(self.nodes.len() as u32);
        let len = node.len + 1;
        let has_elem = node.has_elem || matches!(slot, Slot::Elem(_));
        let has_weak = node.has_weak || !slot.owns();
        self.nodes.push(PathNode {
            parent: path,
            slot: Some(slot),
            len,
            has_elem,
            has_weak,
        });
        self.children.insert((path, slot), child);
        child
    }

    /// Whether the path crosses a weak reference, so its object need not be beneath the root.
    pub(super) fn has_weak(&self, path: PathId) -> bool {
        path.is_exact() && self.nodes[path.0 as usize].has_weak
    }

    pub(super) fn slots(&self, mut path: PathId) -> SmallVec<[Slot; 4]> {
        let mut out = SmallVec::new();
        while path != PathId::ROOT && path.is_exact() {
            let node = &self.nodes[path.0 as usize];
            out.extend(node.slot);
            path = node.parent;
        }
        out.reverse();
        out
    }

    pub(super) fn contains(&self, mut path: PathId, slot: Slot) -> bool {
        if !path.is_exact() {
            return false;
        }
        while path != PathId::ROOT {
            let node = &self.nodes[path.0 as usize];
            if node.slot == Some(slot) {
                return true;
            }
            path = node.parent;
        }
        false
    }

    /// True when `short` names an object that reached the object at `long` when it was read.
    /// Element steps are excluded on the short side: `a[i]` does not reach `a[j].x`.
    pub(super) fn strict_ancestor(&self, short: PathId, mut long: PathId) -> bool {
        if long.is_detached() {
            return short == PathId::ROOT;
        }
        if !short.is_exact() || !long.is_exact() {
            return false;
        }
        let short_node = &self.nodes[short.0 as usize];
        if short_node.has_elem {
            return false;
        }
        let target = short_node.len;
        let long_node = &self.nodes[long.0 as usize];
        if long_node.len <= target || long_node.has_weak {
            return false;
        }
        while self.nodes[long.0 as usize].len > target {
            long = self.nodes[long.0 as usize].parent;
        }
        long == short
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(super) enum Atom {
    /// Any object that existed before the analyzed code ran and is not derived from a parameter.
    Old,
    Param(u32, PathId),
    /// An allocation site in the current function. In summaries, `Fresh(g)` names every object
    /// the callee allocated in group `g` (see [`super::summary::FreshGroup`]).
    Fresh(u32),
}

impl Atom {
    pub(super) fn is_param(self) -> bool {
        matches!(self, Atom::Param(..))
    }
}

pub(super) type Atoms = SmallVec<[Atom; 4]>;

pub(super) fn insert(set: &mut Atoms, atom: Atom) -> bool {
    match set.binary_search(&atom) {
        Ok(_) => false,
        Err(at) => {
            set.insert(at, atom);
            true
        }
    }
}

pub(super) fn union(set: &mut Atoms, other: &[Atom]) -> bool {
    let mut changed = false;
    for &atom in other {
        changed |= insert(set, atom);
    }
    changed
}

pub(super) fn one(atom: Atom) -> Atoms {
    let mut set = Atoms::new();
    set.push(atom);
    set
}
