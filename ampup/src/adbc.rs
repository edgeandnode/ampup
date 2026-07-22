//! ADBC driver catalog and release-asset naming.
//!
//! Amp releases ship a pinned set of ADBC driver libraries as archive assets.
//! This module maps a supported driver plus a target platform/arch to the
//! release asset that carries it.

use std::path::Path;

use serde::Serialize;

use crate::platform::{Architecture, Platform};

/// A supported ADBC driver, as shipped in Amp releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    Postgresql,
}

impl Driver {
    /// Every supported driver.
    pub const ALL: &'static [Driver] = &[Driver::Postgresql];

    /// The driver's canonical name (the `<driver>` segment in asset names).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Postgresql => "postgresql",
        }
    }

    /// Parse a driver from its canonical name, or `None` if unsupported.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|d| d.as_str() == name)
    }

    /// The runtime library filename this driver's archive carries on
    /// `platform` (e.g. `libadbc_driver_postgresql.so` on Linux).
    pub fn runtime_lib_filename(&self, platform: Platform) -> String {
        let ext = match platform {
            Platform::Linux => "so",
            Platform::Darwin => "dylib",
        };
        format!("libadbc_driver_{}.{ext}", self.as_str())
    }
}

impl std::fmt::Display for Driver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// The GitHub release asset name for a driver on a given target.
///
/// Must match the naming the release pipeline produces (edgeandnode/amp #2599):
/// `adbc-driver-<driver>-<platform>-<arch>.tar.gz`.
pub fn asset_name(driver: Driver, platform: Platform, arch: Architecture) -> String {
    format!(
        "adbc-driver-{}-{}-{}.tar.gz",
        driver.as_str(),
        platform.as_str(),
        arch.as_str(),
    )
}

/// An ADBC driver manifest (`manifest.toml`), the on-disk contract `ampd`
/// reads to load a driver by absolute path.
///
/// Only `Driver.shared` is required; the postgres driver's entrypoint is
/// derived from its name, so no `entrypoint` key is emitted.
#[derive(Serialize)]
struct Manifest {
    manifest_version: u32,
    #[serde(rename = "Driver")]
    driver: DriverSection,
}

#[derive(Serialize)]
struct DriverSection {
    shared: String,
}

/// Render the `manifest.toml` contents pointing `ampd` at the driver library
/// at `lib_path` (an absolute path).
pub fn driver_manifest(lib_path: &Path) -> String {
    let manifest = Manifest {
        manifest_version: 1,
        driver: DriverSection {
            shared: lib_path.to_string_lossy().into_owned(),
        },
    };
    // Serializing a fixed two-field struct cannot fail.
    toml::to_string(&manifest).expect("driver manifest serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_matches_release_contract() {
        assert_eq!(
            asset_name(Driver::Postgresql, Platform::Linux, Architecture::X86_64),
            "adbc-driver-postgresql-linux-x86_64.tar.gz",
        );
        assert_eq!(
            asset_name(Driver::Postgresql, Platform::Darwin, Architecture::Aarch64),
            "adbc-driver-postgresql-darwin-aarch64.tar.gz",
        );
    }

    #[test]
    fn from_name_accepts_supported_and_rejects_unknown() {
        assert_eq!(Driver::from_name("postgresql"), Some(Driver::Postgresql));
        assert_eq!(Driver::from_name("mysql"), None);
        assert_eq!(Driver::from_name(""), None);
    }

    #[test]
    fn runtime_lib_filename_is_platform_specific() {
        assert_eq!(
            Driver::Postgresql.runtime_lib_filename(Platform::Linux),
            "libadbc_driver_postgresql.so",
        );
        assert_eq!(
            Driver::Postgresql.runtime_lib_filename(Platform::Darwin),
            "libadbc_driver_postgresql.dylib",
        );
    }

    #[test]
    fn driver_manifest_round_trips_the_library_path() {
        // A path with a quote and a backslash must survive escaping so the
        // manifest stays valid TOML that parses back to the same path.
        let path = Path::new("/home/a\"b\\c/drivers/postgresql/libadbc_driver_postgresql.so");
        let rendered = driver_manifest(path);

        let parsed: toml::Value = toml::from_str(&rendered).expect("manifest is valid TOML");
        assert_eq!(parsed["manifest_version"].as_integer(), Some(1));
        assert_eq!(
            parsed["Driver"]["shared"].as_str(),
            Some(path.to_string_lossy().as_ref()),
        );
    }
}
