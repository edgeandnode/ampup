//! Optional ADBC driver components (#2600).
//!
//! Installs destination-specific ADBC driver libraries from the pinned set
//! shipped with each Amp release, so `ampd` can load them at runtime.

use anyhow::{Result, anyhow, bail};

use crate::adbc::Driver;

/// Install an ADBC driver for the active amp version.
pub async fn install(driver: &str) -> Result<()> {
    let driver = parse_driver(driver)?;
    bail!("`ampup adbc install {driver}` is not yet implemented");
}

/// List installed ADBC drivers for the active version.
pub fn list() -> Result<()> {
    bail!("`ampup adbc list` is not yet implemented");
}

/// Uninstall an ADBC driver.
pub fn uninstall(driver: &str) -> Result<()> {
    let driver = parse_driver(driver)?;
    bail!("`ampup adbc uninstall {driver}` is not yet implemented");
}

/// Resolve a driver name against the catalog, with a helpful error listing the
/// supported drivers.
fn parse_driver(name: &str) -> Result<Driver> {
    Driver::from_name(name).ok_or_else(|| {
        let supported = Driver::ALL
            .iter()
            .map(Driver::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        anyhow!("unknown ADBC driver `{name}` (supported: {supported})")
    })
}
