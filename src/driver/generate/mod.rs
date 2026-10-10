//! Compile-time source generators: `@generator fun name(ctx: GenContext)` Dream functions
//! triggered by `@on_attribute`, `@on_call` and `@syntax_block`, run as cached debuggable
//! executables on a JSON snapshot of the program, with results merged back as generated files
//! and syntax-site replacements.

mod apply;
mod call_sites;
#[cfg(feature = "native")]
pub mod commands;
mod decls;
mod exe;
#[cfg(feature = "native")]
mod execute;
mod identity;
mod incremental;
mod inspect;
mod manifest;
mod materialize;
mod merge;
mod model;
#[cfg(feature = "native")]
mod parallel;
mod pass;
mod paths;
mod registry;
mod rewrite;
#[cfg(feature = "native")]
mod run;
mod sites;
mod snapshot;
mod stage;
mod stats;
mod type_ref;
mod walk;

pub use decls::module_of;
#[cfg(feature = "native")]
pub use exe::ensure_built;
pub use exe::{ExePlan, GenExe, group as group_executables, plan as plan_executable};
pub use identity::compiler_identity;
pub use inspect::{GenInspection, InspectedGenerator, inspect};
pub use manifest::{default_compile_entry, find_project_root, find_project_root_from};
pub use materialize::{header as generated_header, materialized_text};
pub use model::{GenResult, SNAPSHOT_VERSION, Snapshot};
pub use pass::{GenerateRequest, PassInputs, gather, run_generators};
pub use registry::RegisteredGenerator;
#[cfg(feature = "native")]
pub use run::{DEFAULT_TIMEOUT_SECS, exe_args, run as run_executable};
pub use stage::GeneratorStage;
pub use stats::GenStats;
