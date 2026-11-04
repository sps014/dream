//! Flow state: which tracked locals are moved at a program point.

use dream_hir::{HSpan, LocalId};
use indexmap::IndexMap;

/// Where a moved value went, for the note on a later use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Destination {
    /// Passed to parameter `param` of `callee` (a function, method or constructor), which keeps it.
    Argument { callee: String, param: String },
    /// Bound to another local.
    Local(String),
    /// Stored into a field or element, with its source spelling when it has a simple one.
    Place(Option<String>),
    /// Placed into an array, tuple or union value.
    Aggregate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Move {
    pub(super) span: HSpan,
    pub(super) to: Destination,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Status {
    Moved(Move),
    /// Moved on some paths reaching this point but not others.
    MaybeMoved(Move),
}

impl Status {
    pub(super) fn site(&self) -> &Move {
        match self {
            Status::Moved(m) | Status::MaybeMoved(m) => m,
        }
    }
}

/// Moved locals at one program point. A local missing from `moved` owns its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct State {
    pub(super) reachable: bool,
    pub(super) moved: IndexMap<LocalId, Status>,
}

impl State {
    pub(super) fn entry() -> Self {
        State {
            reachable: true,
            moved: IndexMap::new(),
        }
    }

    pub(super) fn unreachable() -> Self {
        State {
            reachable: false,
            moved: IndexMap::new(),
        }
    }

    /// The state where two control-flow paths meet.
    pub(super) fn join(&self, other: &State) -> State {
        if !self.reachable {
            return other.clone();
        }
        if !other.reachable {
            return self.clone();
        }
        let mut moved = IndexMap::new();
        for (local, status) in &self.moved {
            let joined = match (status, other.moved.get(local)) {
                (Status::Moved(m), Some(Status::Moved(_))) => Status::Moved(m.clone()),
                (status, _) => Status::MaybeMoved(status.site().clone()),
            };
            moved.insert(*local, joined);
        }
        for (local, status) in &other.moved {
            moved
                .entry(*local)
                .or_insert_with(|| Status::MaybeMoved(status.site().clone()));
        }
        State {
            reachable: true,
            moved,
        }
    }
}
