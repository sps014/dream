//! Native GPU capability on slang-rhi, loading only precompiled shader bundles: the Slang
//! compiler is never linked or loaded. Guest allocation, binding and future completion stay
//! owned by the core host. See `docs/internals/19-gpu-host.md`.
pub mod backend;
pub mod bundle;
mod commands;
pub mod devices;
pub mod error;
mod exports;
mod ffi;
pub mod guest;
mod handles;
pub mod pipelines;
pub mod raytracing;
pub mod resources;
mod state;
mod strict_aot;
pub mod submit;
pub mod surfaces;
pub mod testing;
mod waiter;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_gpu_abi_v2() {}
