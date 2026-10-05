//! Builtin `system.webapi` dispatcher: `@get`/`@post`/… + extractors + `@dep` → `extend WebApp`.

mod analysis;
mod attributes;
mod dependencies;
mod emit;
mod expand;
mod extractors;
mod model;
mod openapi;
mod types;

mod collect;

pub use expand::expand_from_acc;
