//! Elision certificates live only until the original statement snapshot is rewritten.

use super::*;

pub(super) struct Proof {
    local: u32,
    candidate: Candidate,
    ignored: IndexSet<u32>,
    operations: Vec<(usize, usize)>,
}

impl Proof {
    pub(super) fn record(
        f: &MirFunction,
        local: u32,
        candidate: &Candidate,
        ignored: &IndexSet<u32>,
    ) -> Self {
        Self {
            local,
            candidate: candidate.clone(),
            ignored: ignored.clone(),
            operations: operations(f, &candidate.class),
        }
    }

    #[cfg(test)]
    fn validate(
        &self,
        f: &MirFunction,
        i: &TypeInterner,
        layouts: &LayoutTable,
        effects: &ModRefTable,
    ) -> bool {
        let current = candidates(f, i, layouts);
        let facts = OwnershipFacts::compute(f, i);
        let live = liveness::live_out(&without_rc_on(f, &self.ignored));
        let token_live = liveness::live_out(f);
        let context = Check {
            token_live: &token_live,
            f,
            interner: i,
            layouts,
            modref: effects,
            x: self.local,
            snap: self.candidate.snap,
            class: &self.candidate.class,
            ignored: &self.ignored,
            facts: &facts,
            observes: crate::analysis::ownership::observes_ownership(f, i, layouts, effects),
        };
        self.validate_with(&context, &live, &current)
    }

    pub(super) fn validate_with(
        &self,
        context: &Check<'_>,
        live: &[IndexSet<u32>],
        current: &BTreeMap<u32, Candidate>,
    ) -> bool {
        let Some(candidate) = current.get(&self.local) else {
            return false;
        };
        if candidate.snap != self.candidate.snap
            || candidate.class != self.candidate.class
            || context.x != self.local
            || context.snap != self.candidate.snap
            || context.class != &self.candidate.class
            || context.ignored != &self.ignored
            || operations(context.f, &candidate.class) != self.operations
        {
            return false;
        }
        context.holds(live)
    }
}

fn operations(f: &MirFunction, class: &BTreeSet<u32>) -> Vec<(usize, usize)> {
    f.blocks
        .iter()
        .enumerate()
        .flat_map(|(bi, b)| {
            b.stmts.iter().enumerate().filter_map(move |(si, s)| {
                rc_target(s)
                    .is_some_and(|l| class.contains(&l))
                    .then_some((bi, si))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::FunctionBuilder;

    #[test]
    fn certificate_rejects_owner_death_publication_and_changed_operations() {
        let i = TypeInterner::new();
        let mut b = FunctionBuilder::new("certificate", i.void());
        let owner = b.new_param(i.string(), None);
        let alias = b.new_local(i.string(), None);
        let scalar = b.new_local(i.int(), None);
        let op = |l| Operand::Copy(Place::Local(l));
        b.assign(Place::Local(alias), Rvalue::Use(op(owner)));
        b.push(Statement::Retain(op(alias)));
        b.push(Statement::Nop);
        b.assign(Place::Local(scalar), Rvalue::StrLen(op(alias)));
        b.push(Statement::Release(op(alias)));
        b.terminate(Terminator::Return(None));
        let mut f = b.finish();
        let layouts = LayoutTable::default();
        let effects = ModRefTable::compute(&Mir::default(), &i);
        let c = candidates(&f, &i, &layouts)
            .remove(&alias.0)
            .expect("candidate");
        let proof = Proof::record(&f, alias.0, &c, &IndexSet::from([alias.0]));
        assert!(proof.validate(&f, &i, &layouts, &effects));
        f.blocks[0].stmts[2] = Statement::Release(op(owner));
        assert!(!proof.validate(&f, &i, &layouts, &effects));
        f.blocks[0].stmts[2] = Statement::Nop;
        f.blocks[0].terminator = Terminator::Return(Some(op(alias)));
        assert!(!proof.validate(&f, &i, &layouts, &effects));
        f.blocks[0].terminator = Terminator::Return(None);
        f.blocks[0].stmts[1] = Statement::Nop;
        assert!(!proof.validate(&f, &i, &layouts, &effects));
    }
}
