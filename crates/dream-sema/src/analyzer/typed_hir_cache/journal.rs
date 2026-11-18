//! The program-wide requests a body makes while it is analyzed, so a reused body can repeat them.
//!
//! Instantiating a generic type or function is the only program-wide change a cacheable body may
//! cause. Each outermost request is recorded together with the types interned between requests,
//! in the order analysis made them; nested requests belong to the outer one. A body is persisted
//! only when the growth of every program-wide table equals what its recorded requests and
//! interned types account for.

use crate::analyzer::{Analyzer, GenericBindings};
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::FunctionNode;
use dream_types::{DefId, TypeId};

pub(in crate::analyzer) enum Request {
    Struct(DefId, Vec<TypeId>),
    Union(DefId, Vec<TypeId>),
    Interface(DefId, Vec<TypeId>),
    Array(TypeId),
    Function(DefId, Vec<(String, TypeId)>),
}

pub(super) enum Entry {
    Intern(TypeId),
    Request(Request),
}

pub(in crate::analyzer) struct Journal {
    pub(super) entries: Vec<Entry>,
    depth: usize,
    interned: usize,
    /// Table growth inside recorded requests, parallel to [`Analyzer::effect_fingerprint`].
    explained: Vec<usize>,
    unexplained: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) enum Event {
    Intern(super::TypeKey),
    Struct(super::keys::DefKey, Vec<super::TypeKey>),
    Union(super::keys::DefKey, Vec<super::TypeKey>),
    Interface(super::keys::DefKey, Vec<super::TypeKey>),
    Array(super::TypeKey),
    Function(super::keys::DefKey, Vec<(String, super::TypeKey)>),
}

impl<'a> Analyzer<'a> {
    pub(super) fn start_journal(&mut self) -> Vec<usize> {
        let before = self.effect_fingerprint();
        self.journal = Some(Journal {
            entries: Vec::new(),
            depth: 0,
            interned: self.type_ctx.interner.len(),
            explained: vec![0; before.len()],
            unexplained: false,
        });
        before
    }

    /// Ends recording and returns the entries when they account for all growth since `before`.
    /// The body's own HIR function is the one growth allowed beyond them.
    pub(super) fn finish_journal(&mut self, before: &[usize]) -> Option<Vec<Entry>> {
        self.flush_interned();
        let journal = self.journal.take()?;
        if journal.unexplained {
            return None;
        }
        let after = self.effect_fingerprint();
        let interned = journal
            .entries
            .iter()
            .filter(|entry| matches!(entry, Entry::Intern(_)))
            .count();
        let last = after.len() - 1;
        let accounted = after.iter().zip(before).enumerate().all(|(slot, (&a, &b))| {
            let own = match slot {
                0 => interned,
                _ if slot == last => 1,
                _ => 0,
            };
            a.checked_sub(b) == Some(journal.explained[slot] + own)
        });
        accounted.then_some(journal.entries)
    }

    /// Runs `run`, recording `request` when it is the outermost request of a recorded body.
    /// A `None` request marks the body as not reusable.
    pub(in crate::analyzer) fn journaled<R>(
        &mut self,
        request: impl FnOnce(&Self) -> Option<Request>,
        run: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let outermost = match &mut self.journal {
            Some(journal) => {
                journal.depth += 1;
                journal.depth == 1
            }
            None => return run(self),
        };
        let before = if outermost {
            self.flush_interned();
            let request = request(self);
            if let Some(journal) = &mut self.journal {
                match request {
                    Some(request) => journal.entries.push(Entry::Request(request)),
                    None => journal.unexplained = true,
                }
            }
            Some(self.effect_fingerprint())
        } else {
            None
        };
        let result = run(self);
        let after = before.as_ref().map(|_| self.effect_fingerprint());
        let interned = self.type_ctx.interner.len();
        if let Some(journal) = &mut self.journal {
            journal.depth -= 1;
            if let (Some(before), Some(after)) = (before, after) {
                for (sum, (b, a)) in journal.explained.iter_mut().zip(before.iter().zip(&after)) {
                    match a.checked_sub(*b) {
                        Some(growth) => *sum += growth,
                        None => journal.unexplained = true,
                    }
                }
                journal.interned = interned;
            }
        }
        result
    }

    fn flush_interned(&mut self) {
        let len = self.type_ctx.interner.len();
        if let Some(journal) = &mut self.journal {
            journal
                .entries
                .extend((journal.interned..len).map(|id| Entry::Intern(TypeId(id as u32))));
            journal.interned = len;
        }
    }

    pub(super) fn encode_events(&self, entries: &[Entry]) -> Vec<Event> {
        let keys = |ids: &[TypeId]| ids.iter().map(|&id| self.type_key(id)).collect();
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Intern(ty) => Event::Intern(self.type_key(*ty)),
                Entry::Request(Request::Struct(def, args)) => {
                    Event::Struct(self.def_key(*def), keys(args))
                }
                Entry::Request(Request::Union(def, args)) => {
                    Event::Union(self.def_key(*def), keys(args))
                }
                Entry::Request(Request::Interface(def, args)) => {
                    Event::Interface(self.def_key(*def), keys(args))
                }
                Entry::Request(Request::Array(ty)) => Event::Array(self.type_key(*ty)),
                Entry::Request(Request::Function(def, bindings)) => Event::Function(
                    self.def_key(*def),
                    bindings
                        .iter()
                        .map(|(name, ty)| (name.clone(), self.type_key(*ty)))
                        .collect(),
                ),
            })
            .collect()
    }

    /// Repeats a stored body's requests in order. Returns `None` at the first event that no longer
    /// resolves; the requests already repeated are idempotent and remain valid.
    pub(super) fn replay_events(
        &mut self,
        events: &[Event],
        diagnostics: &mut DiagnosticBag,
    ) -> Option<()> {
        let span = crate::analyzer::empty_span();
        for event in events {
            self.refresh_def_index();
            let resolve_all = |s: &Self, keys: &[super::TypeKey]| -> Option<Vec<TypeId>> {
                keys.iter().map(|key| s.resolve_type(key)).collect()
            };
            match event {
                Event::Intern(key) => {
                    self.intern_type_key(key)?;
                }
                Event::Struct(def, args) => {
                    let (def, args) = (self.resolve_def(def)?, resolve_all(self, args)?);
                    let name = self.type_ctx.defs.name(def).to_string();
                    self.ensure_struct_instance(def, &name, args, &span, diagnostics);
                }
                Event::Union(def, args) => {
                    let (def, args) = (self.resolve_def(def)?, resolve_all(self, args)?);
                    let name = self.type_ctx.defs.name(def).to_string();
                    self.ensure_union_instance(def, &name, args, &span, diagnostics);
                }
                Event::Interface(def, args) => {
                    let (def, args) = (self.resolve_def(def)?, resolve_all(self, args)?);
                    let name = self.type_ctx.defs.name(def).to_string();
                    self.ensure_interface_instance(def, &name, args, &span, diagnostics);
                }
                Event::Array(key) => {
                    let ty = self.resolve_type(key)?;
                    self.ensure_array_collection(ty, diagnostics);
                }
                Event::Function(def, bindings) => {
                    let def = self.resolve_def(def)?;
                    let template: &'a FunctionNode<'a> = self.generic_functions.get(&def)?;
                    let mut generic = GenericBindings::new();
                    let mut args = Vec::new();
                    for (name, key) in bindings {
                        let ty = self.resolve_type(key)?;
                        generic.insert(name.clone(), self.type_ctx.syntax_type(ty));
                        args.push(ty);
                    }
                    let scope = self.type_ctx.scope();
                    self.type_ctx
                        .set_scope(self.graph.module_for_file(template.file_path.as_deref()));
                    self.instantiate_generic_function(template, &generic, &(def, args));
                    self.type_ctx.set_scope(scope);
                }
            }
        }
        Some(())
    }
}
