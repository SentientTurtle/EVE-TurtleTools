#![cfg_attr(feature="docs_export", feature(proc_macro_hygiene))]
#![cfg_attr(feature="docs_export", feature(custom_inner_attributes))]

pub mod types;
pub mod util;
pub mod sde;

pub mod hardcoded;

#[cfg(test)]
pub mod test;

pub const CRATE_NAME: &'static str = env!("CARGO_PKG_NAME");
pub const CRATE_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const CRATE_REPO: &'static str = env!("CARGO_PKG_REPOSITORY");
/// Version of the SDE this crate version was designed for, effectively a "minimum" version that this crate will parse successfully
pub const SDE_DESIGN_VERSION: &'static str = "3484357";