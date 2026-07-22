//! Optional ADBC driver components (#2600).
//!
//! Installs destination-specific ADBC driver libraries from the pinned set
//! shipped with each Amp release, so `ampd` can load them at runtime.

use anyhow::{Result, bail};

/// Install an ADBC driver for the active amp version.
pub async fn install(driver: &str) -> Result<()> {
    bail!("`ampup adbc install {driver}` is not yet implemented");
}

/// List installed ADBC drivers for the active version.
pub fn list() -> Result<()> {
    bail!("`ampup adbc list` is not yet implemented");
}

/// Uninstall an ADBC driver.
pub fn uninstall(driver: &str) -> Result<()> {
    bail!("`ampup adbc uninstall {driver}` is not yet implemented");
}
