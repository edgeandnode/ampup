//! ADBC driver catalog and release-asset naming.
//!
//! Amp releases ship a pinned set of ADBC driver libraries as archive assets.
//! This module maps a supported driver plus a target platform/arch to the
//! release asset that carries it.

use crate::platform::{Architecture, Platform};

/// Prefix every installed driver library carries, namespacing it against the
/// amp binaries it sits beside in a version directory.
pub const DRIVER_FILE_PREFIX: &str = "amp-adbc-driver-";

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

    /// The library filename this driver's release archive carries on
    /// `platform` (e.g. `libadbc_driver_postgresql.so` on Linux) — upstream's
    /// own name, which the archive is validated against.
    pub fn archive_lib_filename(&self, platform: Platform) -> String {
        format!(
            "libadbc_driver_{}.{}",
            self.as_str(),
            lib_extension(platform)
        )
    }

    /// The library filename this driver is installed under, prefixed so it is
    /// distinguishable from the amp binaries sharing its directory.
    pub fn installed_lib_filename(&self, platform: Platform) -> String {
        format!(
            "{DRIVER_FILE_PREFIX}{}.{}",
            self.as_str(),
            lib_extension(platform)
        )
    }

    /// The stem shared by this driver's installed files, used for the license
    /// sidecars. Platform-independent, so cleanup needs no platform.
    pub fn installed_stem(&self) -> String {
        format!("{DRIVER_FILE_PREFIX}{}", self.as_str())
    }
}

/// The dynamic library extension for `platform`.
fn lib_extension(platform: Platform) -> &'static str {
    match platform {
        Platform::Linux => "so",
        Platform::Darwin => "dylib",
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
    fn archive_lib_filename_is_the_upstream_platform_specific_name() {
        assert_eq!(
            Driver::Postgresql.archive_lib_filename(Platform::Linux),
            "libadbc_driver_postgresql.so",
        );
        assert_eq!(
            Driver::Postgresql.archive_lib_filename(Platform::Darwin),
            "libadbc_driver_postgresql.dylib",
        );
    }

    #[test]
    fn installed_lib_filename_is_prefixed_and_distinct_from_the_archive_name() {
        for platform in Platform::ALL.iter().copied() {
            let installed = Driver::Postgresql.installed_lib_filename(platform);
            assert!(
                installed.starts_with(DRIVER_FILE_PREFIX),
                "installed name namespaces the driver: {installed}",
            );
            assert_ne!(
                installed,
                Driver::Postgresql.archive_lib_filename(platform),
                "the driver is renamed on install",
            );
        }
        assert_eq!(
            Driver::Postgresql.installed_lib_filename(Platform::Darwin),
            "amp-adbc-driver-postgresql.dylib",
        );
    }

    #[test]
    fn installed_stem_prefixes_every_installed_file() {
        let stem = Driver::Postgresql.installed_stem();
        assert_eq!(stem, "amp-adbc-driver-postgresql");
        // The sidecars and the library share the stem, so cleanup by stem
        // catches all of them without knowing the platform.
        assert!(
            Driver::Postgresql
                .installed_lib_filename(Platform::Linux)
                .starts_with(&stem)
        );
    }
}
