// These C entry points share the guest ABI contract: pointer handles come from
// the validated program's runtime, never arbitrary Rust callers.
#![allow(clippy::missing_safety_doc)]

mod abi;
mod app_icon;
