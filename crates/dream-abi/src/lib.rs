//! Shared compiler ABI constants and registries used by both semantic analysis and MIR.
//!
//! Lives outside `dream-mir` so `dream-sema` can use JS interop names / intrinsics / attributes
//! without depending on the backend.

pub mod attributes;
pub mod c_abi;
pub mod capabilities;
pub mod exports;
pub mod gpu_bindings;
pub mod guest_contract;
pub mod host_capability;
pub mod intrinsics;
pub mod iterators;
pub mod js_abi;
pub mod library;
pub mod names;
pub mod profile;
pub mod runtime_hosts;
pub mod target;
pub mod toolchain;
