use crate::MirFunction;
use std::cell::Cell;

#[derive(Clone, Copy)]
pub(super) enum Limit {
    Function,
    Inline,
    RcElision,
}

thread_local! {
    static HITS: Cell<[u64; 3]> = const { Cell::new([0; 3]) };
}

pub(super) fn reached(limit: Limit, iterations: usize, function: Option<&MirFunction>) {
    let hits = HITS.with(|counts| {
        let mut values = counts.get();
        values[limit as usize] += 1;
        counts.set(values);
        values[limit as usize]
    });
    let stage = match limit {
        Limit::Function => "function-fixpoint",
        Limit::Inline => "module-inline",
        Limit::RcElision => "rc-elision",
    };
    tracing::info!(
        stage,
        iterations,
        hits,
        function = function.map(|f| f.name.as_str()),
        "MIR iteration cap reached while the last round still changed the program"
    );
}

#[cfg(test)]
pub(super) fn hits(limit: Limit) -> u64 {
    HITS.with(|counts| counts.get()[limit as usize])
}
