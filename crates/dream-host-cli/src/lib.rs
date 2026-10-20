//! Native CLI parsing backed by clap.
mod command;
mod exports;
mod parse;
mod schema;

#[unsafe(no_mangle)]
pub extern "C" fn dream_host_cli_abi_v3() {}

#[cfg(test)]
mod tests;
