//! `--emit-mir` snapshot sink threaded through [`super::optimize_module_opts`],
//! [`super::PassManager::run_dumped`], and [`super::run_late_module_passes`].
//!
//! The sink only collects `(file name, text)` pairs; the driver owns the output directory. File
//! numbering is assigned once at [`MirDump::finish`] with a width wide enough that lexicographic
//! order equals emission order.

use super::{MirPass, PassManager};
use crate::pretty::{print_function, MirNames, PrettyCx};
use crate::{Mir, MirFunction};
use dream_types::{DefId, TypeId, TypeInterner};
use indexmap::IndexMap;
use std::fmt::Write;

/// The MIR as lowered, before any module pass.
pub const STAGE_LOWER: &str = "lower";
/// After every function (and poll body) reached its per-function [`PassManager`] fixpoint.
pub const STAGE_FIXPOINT: &str = "fixpoint";
/// After [`super::run_late_module_passes`].
pub const STAGE_LATE: &str = "strip-escaped-regions";

/// Module-level stages in pipeline order (names of the passes run by `optimize_module_opts`).
const MODULE_STAGES: &[&str] = &[
    STAGE_LOWER,
    "expand-simple-ctors",
    "funcbox-abi",
    "param-modes",
    "rc-insertion",
    "devirt",
    "inline",
    "rc-last-use-repair",
    "unique-region",
    super::rc::held::STAGE,
    "sroa-managed",
    super::slice_measure::STAGE,
    STAGE_FIXPOINT,
    STAGE_LATE,
    super::frame_alloc::STAGE,
];

/// Every name `--emit-mir=after:<pass>` accepts: module stages in pipeline order, then every
/// per-function pass of any shipped [`PassManager`] pipeline, sorted.
pub fn dumpable_pass_names() -> Vec<&'static str> {
    let mut fn_passes: Vec<&'static str> = [
        PassManager::default_pipeline(),
        PassManager::native_c_pipeline(),
        PassManager::async_poll_pipeline(),
        PassManager::debug_pipeline(),
    ]
    .iter()
    .flat_map(|pm| pm.passes.iter().map(|p| p.name()))
    .filter(|n| !MODULE_STAGES.contains(n))
    .collect();
    fn_passes.sort_unstable();
    fn_passes.dedup();
    MODULE_STAGES.iter().copied().chain(fn_passes).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DumpMode {
    After { pass: &'static str, each: bool },
    All,
}

/// A parsed `--emit-mir` / `--emit-mir-fn` request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirDumpSpec {
    mode: DumpMode,
    fns: Vec<String>,
}

impl MirDumpSpec {
    /// Parses `all`, `after:<pass>`, or `after:<pass>,each`, plus an optional comma-separated list
    /// of exact function names. The error text lists the valid pass names.
    pub fn parse(spec: &str, fns: Option<&str>) -> Result<Self, String> {
        let mode = if spec == "all" {
            DumpMode::All
        } else if let Some(rest) = spec.strip_prefix("after:") {
            let (name, each) = match rest.split_once(',') {
                Some((name, "each")) => (name, true),
                Some((_, other)) => {
                    return Err(format!(
                        "--emit-mir: unknown modifier `{other}` (only `each` is supported)"
                    ))
                }
                None => (rest, false),
            };
            let valid = dumpable_pass_names();
            let Some(pass) = valid.iter().copied().find(|p| *p == name) else {
                return Err(format!(
                    "--emit-mir: unknown pass `{name}`; valid passes: {}",
                    valid.join(", ")
                ));
            };
            DumpMode::After { pass, each }
        } else {
            return Err(format!(
                "--emit-mir: expected `all`, `after:<pass>`, or `after:<pass>,each`, got `{spec}`"
            ));
        };
        let fns = fns
            .map(|list| {
                list.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Ok(MirDumpSpec { mode, fns })
    }

    fn keeps(&self, f: &MirFunction) -> bool {
        self.fns.is_empty() || self.fns.contains(&f.name)
    }
}

/// One snapshot file, named `<NN>-<pass>.mir` relative to the dump directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirDumpFile {
    pub name: String,
    pub contents: String,
}

struct Snapshot {
    pass: &'static str,
    text: String,
}

/// Collects MIR snapshots for one compile. [`MirDump::disabled`] makes every hook a no-op.
#[derive(Default)]
pub struct MirDump {
    spec: Option<MirDumpSpec>,
    names: MirNames,
    snapshots: Vec<Snapshot>,
    module_runs: IndexMap<&'static str, usize>,
    last_module: Option<Snapshot>,
    last_fn: IndexMap<(DefId, Vec<TypeId>, bool), String>,
}

impl MirDump {
    pub fn disabled() -> Self {
        MirDump::default()
    }

    pub fn new(spec: MirDumpSpec) -> Self {
        MirDump {
            spec: Some(spec),
            ..MirDump::default()
        }
    }

    pub fn is_active(&self) -> bool {
        self.spec.is_some()
    }

    /// Records a module-level stage. Every stage name must be one of `MODULE_STAGES`.
    pub fn module(&mut self, stage: &'static str, mir: &Mir, interner: &TypeInterner) {
        let Some(spec) = &self.spec else {
            return;
        };
        debug_assert!(MODULE_STAGES.contains(&stage), "unregistered stage {}", stage);
        let run = {
            let r = self.module_runs.entry(stage).or_insert(0);
            *r += 1;
            *r
        };
        let wanted = match &spec.mode {
            DumpMode::All => Some(true),
            DumpMode::After { pass, each } if *pass == stage => Some(*each),
            DumpMode::After { .. } => None,
        };
        self.names = MirNames::of(mir);
        let Some(keep_every_run) = wanted else {
            return;
        };
        let cx = PrettyCx::new(interner, &self.names);
        let mut text = format!("// dream --emit-mir: after {stage} (run {run})\n\n");
        for f in mir.functions.iter().filter(|f| spec.keeps(f)) {
            text.push_str(&print_function(&cx, f));
            text.push('\n');
        }
        for p in mir.polls.iter().filter(|f| spec.keeps(f)) {
            text.push_str("poll ");
            text.push_str(&print_function(&cx, p));
            text.push('\n');
        }
        let snap = Snapshot { pass: stage, text };
        if keep_every_run {
            self.snapshots.push(snap);
        } else {
            self.last_module = Some(snap);
        }
    }

    /// Records one run of a per-function pass inside the [`PassManager`] fixpoint.
    pub(super) fn function_pass(
        &mut self,
        pass: &dyn MirPass,
        iteration: usize,
        changed: bool,
        func: &MirFunction,
        interner: &TypeInterner,
    ) {
        let Some(spec) = &self.spec else {
            return;
        };
        let DumpMode::After { pass: want, each } = spec.mode else {
            return;
        };
        if want != pass.name() || !spec.keeps(func) || (each && !changed) {
            return;
        }
        let cx = PrettyCx::new(interner, &self.names);
        let body = print_function(&cx, func);
        if each {
            let mut text = String::new();
            let _ = writeln!(
                text,
                "// dream --emit-mir: after {want}, fn {}, fixpoint iteration {iteration}\n",
                func.name
            );
            text.push_str(&body);
            self.snapshots.push(Snapshot { pass: want, text });
        } else {
            let text =
                format!("// fn {}: last run at fixpoint iteration {iteration}\n{body}", func.name);
            self.last_fn
                .insert((func.def, func.instance.clone(), func.is_async), text);
        }
    }

    /// Numbered files in emission order. Empty when the requested pass never ran (or matched no
    /// function) in this pipeline.
    pub fn finish(mut self) -> Vec<MirDumpFile> {
        let Some(spec) = self.spec.take() else {
            return Vec::new();
        };
        if let Some(snap) = self.last_module.take() {
            self.snapshots.push(snap);
        }
        if !self.last_fn.is_empty() {
            if let DumpMode::After { pass, .. } = spec.mode {
                let mut text = format!("// dream --emit-mir: after {pass} (last run per function)\n\n");
                for body in self.last_fn.values() {
                    text.push_str(body);
                    text.push('\n');
                }
                self.snapshots.push(Snapshot { pass, text });
            }
        }
        let width = self.snapshots.len().saturating_sub(1).to_string().len().max(2);
        self.snapshots
            .into_iter()
            .enumerate()
            .map(|(i, s)| MirDumpFile {
                name: format!("{i:0width$}-{}.mir", s.pass),
                contents: s.text,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modes_and_rejects_unknown_pass() {
        let s = MirDumpSpec::parse("after:rc-insertion", Some("a, b")).unwrap();
        assert_eq!(
            s.mode,
            DumpMode::After {
                pass: "rc-insertion",
                each: false
            }
        );
        assert_eq!(s.fns, vec!["a".to_string(), "b".to_string()]);
        let s = MirDumpSpec::parse("after:gvn,each", None).unwrap();
        assert_eq!(
            s.mode,
            DumpMode::After {
                pass: "gvn",
                each: true
            }
        );
        assert_eq!(MirDumpSpec::parse("all", None).unwrap().mode, DumpMode::All);
        let err = MirDumpSpec::parse("after:nope", None).unwrap_err();
        assert!(err.contains("unknown pass `nope`") && err.contains("rc-insertion"), "{}", err);
        assert!(MirDumpSpec::parse("after:gvn,twice", None).is_err());
        assert!(MirDumpSpec::parse("before:gvn", None).is_err());
    }

    #[test]
    fn every_module_pass_name_is_registered() {
        use super::super::{
            Devirt, ExpandSimpleCtors, FuncboxAbi, Inliner, ModulePass, RcInsertion,
            RcLastUseRepair, UniqueRegion,
        };
        let names = dumpable_pass_names();
        for n in [
            ExpandSimpleCtors.name(),
            FuncboxAbi.name(),
            Devirt.name(),
            Inliner.name(),
            UniqueRegion.name(),
            MirPass::name(&RcInsertion),
            MirPass::name(&RcLastUseRepair),
        ] {
            assert!(names.contains(&n), "{} missing from {:?}", n, names);
        }
    }
}
