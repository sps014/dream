use super::super::ModulePass;
use super::FnKey;
use super::eligibility::find_site;
use super::graph::address_taken;
use super::graph::count_call_sites;
use super::graph::recursive_set;
use super::splice::perform_inline;
use dream_types::TypeInterner;
use indexmap::IndexMap as HashMap;

/// Stop inlining into a caller once it has grown past this many blocks, to bound code blow-up.
const CALLER_BLOCK_CAP: usize = 4096;
/// Safety cap on inlines performed into a single function per `run` (defends against any unforeseen
/// non-termination; the DAG-only inlining should terminate well before this).
const MAX_INLINES_PER_FN: usize = 4096;

#[derive(Default)]
pub struct Inliner;

impl ModulePass for Inliner {
    fn name(&self) -> &'static str {
        "inline"
    }

    fn run(&self, mir: &mut crate::Mir, interner: &TypeInterner) -> bool {
        let index: HashMap<FnKey, usize> = mir
            .functions
            .iter()
            .enumerate()
            .map(|(i, f)| ((f.def, f.instance.clone()), i))
            .collect();
        let call_counts = count_call_sites(mir);
        let addr_taken = address_taken(mir);
        let recursive = recursive_set(mir, &index);

        let mut changed = false;
        for fi in 0..mir.functions.len() {
            let mut inlined = 0;
            while inlined < MAX_INLINES_PER_FN {
                if mir.functions[fi].blocks.len() > CALLER_BLOCK_CAP {
                    break;
                }
                let Some(site) = find_site(
                    mir,
                    fi,
                    &index,
                    &recursive,
                    &call_counts,
                    &addr_taken,
                    interner,
                ) else {
                    break;
                };
                perform_inline(mir, fi, site, interner);
                changed = true;
                inlined += 1;
            }
        }
        changed
    }
}
