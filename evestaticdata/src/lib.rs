pub mod types;
pub mod util;
#[cfg(feature = "sde")]
pub mod sde;

pub mod hardcoded;

#[cfg(test)]
pub mod test;

pub const CRATE_NAME: &'static str = env!("CARGO_PKG_NAME");
pub const CRATE_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const CRATE_REPO: &'static str = env!("CARGO_PKG_REPOSITORY");
/// Version of the SDE this crate version was designed for, effectively a "minimum" version that this crate will parse successfully
pub const SDE_DESIGN_VERSION: &'static str = "3484357";



#[cfg(test)]
use crate::sde::{SDEError, SDE};
#[test]
fn test_sde_load() -> Result<(), SDEError> {
    if cfg!(feature = "sde_strict") {
        println!("Testing full SDE load [STRICT MODE]");
    } else {
        println!("Testing full SDE load [NORMAL MODE]");
    }
    let mut sde = SDE::initialize("[testrunner]")?.with_cache("./cache");

    sde::load_full_sde(&mut sde)
}