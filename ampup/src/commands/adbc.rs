//! Optional ADBC driver components (#2600).
//!
//! Installs destination-specific ADBC driver libraries from the pinned set
//! shipped with each Amp release, so `ampd` can load them at runtime.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use fs_err as fs;

use crate::{
    adbc::Driver,
    archive,
    config::Config,
    download_manager::{download_with_retry, verify_artifact},
    github::GitHubClient,
    platform::{Architecture, Platform, PlatformError},
    token, ui,
    version_manager::{VersionError, VersionManager},
};

/// Install an ADBC driver for an amp version (the active one by default).
pub async fn install(
    driver: &str,
    install_dir: Option<PathBuf>,
    repo: String,
    github_token: Option<String>,
    arch: Option<String>,
    platform: Option<String>,
    version: Option<String>,
) -> Result<()> {
    let driver = parse_driver(driver)?;
    let config = Config::new(install_dir)?;
    let github = GitHubClient::new(repo, token::resolve_github_token(github_token))?;
    install_driver(&github, config, driver, arch, platform, version).await
}

/// Fetch, verify, extract, and place `driver` for an amp version using an
/// already-constructed GitHub client. Split from [`install`] so tests can
/// inject a client pointed at a mock server.
pub(crate) async fn install_driver(
    github: &GitHubClient,
    config: Config,
    driver: Driver,
    arch: Option<String>,
    platform: Option<String>,
    version: Option<String>,
) -> Result<()> {
    let version_manager = VersionManager::new(config);
    let version = resolve_version(&version_manager, version)?;
    ensure_installed(&version_manager, &version)?;

    let platform = resolve_platform(platform)?;
    let arch = resolve_arch(arch)?;

    let asset_name = crate::adbc::asset_name(driver, platform, arch);
    ui::info!(
        "Installing {} driver for amp {}",
        driver,
        ui::version(&version)
    );

    let assets = github.fetch_release_assets(&version).await?;
    let asset = assets
        .resolve(&asset_name, false)?
        .expect("a required asset resolves to Some or errors");

    let data = download_with_retry(github, &asset).await?;
    // Driver assets always advertise a digest, so a missing one means the
    // release is malformed. Refuse rather than install a library that would be
    // loaded into ampd without its integrity checked.
    let digest = asset.digest.as_deref().ok_or_else(|| {
        anyhow!(
            "release asset {} has no digest; refusing to install an unverified driver",
            asset.name
        )
    })?;
    verify_artifact(&asset.name, &data, Some(digest))
        .context("driver archive failed verification")?;

    // Stage inside the version directory so the final renames stay on one
    // filesystem. TempDir cleans this up if anything below fails.
    let version_dir = version_manager.config().version_dir(&version);
    let staging =
        tempfile::tempdir_in(&version_dir).context("failed to create staging directory")?;

    let archive_lib = driver.archive_lib_filename(platform);
    archive::extract_and_validate(
        &data,
        staging.path(),
        &[archive_lib.as_str(), "LICENSE.txt", "NOTICE.txt"],
    )?;

    let installed_lib = place_driver(staging.path(), &version_dir, driver, platform)?;

    ui::info!(
        "Installed {} driver for amp {} at {}",
        driver,
        ui::version(&version),
        installed_lib.display(),
    );
    Ok(())
}

/// Move the extracted files from `staging` into `version_dir`, renaming the
/// library to its installed (prefixed) name and namespacing the license
/// sidecars alongside it. Returns the installed library path.
///
/// Each move is a rename within one filesystem, which replaces any existing
/// file of that name — so a reinstall overwrites in place. Renaming over a
/// library `ampd` currently has loaded is safe: the open inode survives.
///
/// A failed move rolls back the ones that already landed. Without that, a
/// partial install would leave the library behind and `list` would report a
/// driver whose installation reported an error.
fn place_driver(
    staging: &Path,
    version_dir: &Path,
    driver: Driver,
    platform: Platform,
) -> Result<PathBuf> {
    let stem = driver.installed_stem();
    let moves = [
        (
            driver.archive_lib_filename(platform),
            driver.installed_lib_filename(platform),
        ),
        ("LICENSE.txt".to_string(), format!("{stem}.LICENSE.txt")),
        ("NOTICE.txt".to_string(), format!("{stem}.NOTICE.txt")),
    ];

    let mut placed: Vec<PathBuf> = Vec::with_capacity(moves.len());
    for (from, to) in &moves {
        let destination = version_dir.join(to);
        if let Err(error) = fs::rename(staging.join(from), &destination) {
            for path in &placed {
                let _ = fs::remove_file(path);
            }
            return Err(anyhow::Error::new(error))
                .with_context(|| format!("failed to move {from} into place"));
        }
        placed.push(destination);
    }

    Ok(version_dir.join(driver.installed_lib_filename(platform)))
}

/// List installed ADBC drivers for an amp version (the active one by default).
pub fn list(install_dir: Option<PathBuf>, version: Option<String>) -> Result<()> {
    let config = Config::new(install_dir)?;
    let version_manager = VersionManager::new(config);

    // Without an explicit version, having none active is an empty state rather
    // than an error.
    let version = match version {
        Some(version) => version,
        None => match version_manager.get_current()? {
            Some(version) => version,
            None => {
                ui::info!("No active amp version");
                return Ok(());
            }
        },
    };

    let drivers = installed_drivers(&version_manager.config().version_dir(&version));
    if drivers.is_empty() {
        ui::info!(
            "No ADBC drivers installed for amp {}",
            ui::version(&version)
        );
        return Ok(());
    }

    ui::info!("Installed ADBC drivers for amp {}:", ui::version(&version));
    for driver in drivers {
        println!("    {driver}");
    }
    Ok(())
}

/// Uninstall an ADBC driver from an amp version (the active one by default).
pub fn uninstall(
    install_dir: Option<PathBuf>,
    driver: &str,
    version: Option<String>,
) -> Result<()> {
    let driver = parse_driver(driver)?;

    let config = Config::new(install_dir)?;
    let version_manager = VersionManager::new(config);
    let version = resolve_version(&version_manager, version)?;

    let version_dir = version_manager.config().version_dir(&version);
    let installed = installed_lib_paths(&version_dir, driver);
    if installed.is_empty() {
        bail!(
            "the {driver} driver is not installed for amp {}",
            ui::version(&version)
        );
    }

    // The library plus its license sidecars all share the driver's stem.
    let stem = driver.installed_stem();
    for name in [format!("{stem}.LICENSE.txt"), format!("{stem}.NOTICE.txt")] {
        let sidecar = version_dir.join(name);
        if sidecar.exists() {
            fs::remove_file(&sidecar).context("failed to remove a driver license file")?;
        }
    }
    for lib in installed {
        fs::remove_file(&lib).context("failed to remove the driver library")?;
    }

    ui::info!(
        "Uninstalled {} driver from amp {}",
        driver,
        ui::version(&version)
    );
    Ok(())
}

/// Resolve the amp version to operate on: an explicit one, or the active one.
fn resolve_version(version_manager: &VersionManager, version: Option<String>) -> Result<String> {
    match version {
        Some(version) => Ok(version),
        None => version_manager
            .get_current()?
            .ok_or_else(|| anyhow!("no active amp version; run `ampup install` first")),
    }
}

/// Require `version`'s binaries before placing drivers under it.
///
/// `ampup install <v>` skips all work when `<v>`'s binaries are already there;
/// otherwise it replaces the whole version directory, taking any drivers
/// installed into it. Installing only into versions that are past that
/// short-circuit keeps drivers from being destroyed by a later install.
///
/// Only installation is gated: listing and uninstalling must still work on a
/// version whose binaries have gone missing, or an orphaned driver could never
/// be inspected or removed.
fn ensure_installed(version_manager: &VersionManager, version: &str) -> Result<()> {
    if !version_manager.is_installed(version) {
        return Err(VersionError::NotInstalled {
            version: version.to_string(),
        }
        .into());
    }
    Ok(())
}

/// The installed library paths for `driver` in `version_dir`, across every
/// supported platform.
///
/// Platform-agnostic on purpose: `adbc install --platform` can place a library
/// for a platform other than this host's, and list/uninstall must still see it
/// rather than stranding a file no ampup command can remove.
fn installed_lib_paths(version_dir: &Path, driver: Driver) -> Vec<PathBuf> {
    Platform::ALL
        .iter()
        .map(|platform| version_dir.join(driver.installed_lib_filename(*platform)))
        .filter(|path| path.is_file())
        .collect()
}

/// The catalog drivers currently installed in `version_dir`.
///
/// The directory also holds the amp binaries and each driver's license
/// sidecars, so membership is decided by looking up the catalog's expected
/// filenames rather than by matching names found there. A missing directory
/// yields no drivers, since the per-file checks simply do not match.
fn installed_drivers(version_dir: &Path) -> Vec<Driver> {
    let mut drivers: Vec<Driver> = Driver::ALL
        .iter()
        .copied()
        .filter(|driver| !installed_lib_paths(version_dir, *driver).is_empty())
        .collect();
    drivers.sort_by_key(|d| d.as_str());
    drivers
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

/// Resolve the target platform from an optional `--platform` override.
fn resolve_platform(over: Option<String>) -> Result<Platform> {
    match over {
        Some(p) => match p.as_str() {
            "linux" => Ok(Platform::Linux),
            "darwin" => Ok(Platform::Darwin),
            _ => Err(PlatformError::UnsupportedPlatform { detected: p }.into()),
        },
        None => Ok(Platform::detect()?),
    }
}

/// Resolve the target architecture from an optional `--arch` override.
fn resolve_arch(over: Option<String>) -> Result<Architecture> {
    match over {
        Some(a) => match a.as_str() {
            "x86_64" | "amd64" => Ok(Architecture::X86_64),
            "aarch64" | "arm64" => Ok(Architecture::Aarch64),
            _ => Err(PlatformError::UnsupportedArchitecture { detected: a }.into()),
        },
        None => Ok(Architecture::detect()?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCHIVE_LIB: &str = "libadbc_driver_postgresql.so";
    const INSTALLED_LIB: &str = "amp-adbc-driver-postgresql.so";

    /// A staging directory holding the three files a driver archive carries.
    fn staged_driver(parent: &Path, lib_contents: &[u8]) -> PathBuf {
        let staging = tempfile::tempdir_in(parent).expect("staging dir");
        fs::write(staging.path().join(ARCHIVE_LIB), lib_contents).expect("write lib");
        fs::write(staging.path().join("LICENSE.txt"), b"license").expect("write license");
        fs::write(staging.path().join("NOTICE.txt"), b"notice").expect("write notice");
        staging.keep()
    }

    /// A version directory containing the amp binaries, as a real install has.
    fn version_dir_with_binaries() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("version dir");
        for binary in ["ampd", "ampctl", "ampsql"] {
            fs::write(dir.path().join(binary), b"#!/bin/sh\n").expect("write binary");
        }
        dir
    }

    #[test]
    fn place_driver_renames_the_library_and_namespaces_the_sidecars() {
        let version_dir = version_dir_with_binaries();
        let staging = staged_driver(version_dir.path(), b"ELF");

        let placed = place_driver(
            &staging,
            version_dir.path(),
            Driver::Postgresql,
            Platform::Linux,
        )
        .expect("place");

        assert_eq!(placed, version_dir.path().join(INSTALLED_LIB));
        assert_eq!(fs::read(&placed).expect("lib"), b"ELF");
        assert!(
            version_dir
                .path()
                .join("amp-adbc-driver-postgresql.LICENSE.txt")
                .exists(),
            "license is namespaced so a second driver cannot clobber it",
        );
        assert!(
            version_dir
                .path()
                .join("amp-adbc-driver-postgresql.NOTICE.txt")
                .exists()
        );
        // The upstream name must not survive alongside the renamed library.
        assert!(!version_dir.path().join(ARCHIVE_LIB).exists());
        // The amp binaries are untouched.
        for binary in ["ampd", "ampctl", "ampsql"] {
            assert!(version_dir.path().join(binary).exists(), "{binary} intact");
        }
    }

    #[test]
    fn place_driver_overwrites_a_previous_install() {
        let version_dir = version_dir_with_binaries();

        let first = staged_driver(version_dir.path(), b"old");
        place_driver(
            &first,
            version_dir.path(),
            Driver::Postgresql,
            Platform::Linux,
        )
        .expect("first");

        let second = staged_driver(version_dir.path(), b"new");
        place_driver(
            &second,
            version_dir.path(),
            Driver::Postgresql,
            Platform::Linux,
        )
        .expect("second");

        assert_eq!(
            fs::read(version_dir.path().join(INSTALLED_LIB)).expect("lib"),
            b"new",
        );
    }

    #[test]
    fn installed_drivers_ignores_the_amp_binaries_and_sidecars() {
        let version_dir = version_dir_with_binaries();
        assert!(
            installed_drivers(version_dir.path()).is_empty(),
            "the amp binaries are not drivers",
        );

        // A license sidecar alone must not register as an installed driver.
        fs::write(
            version_dir
                .path()
                .join("amp-adbc-driver-postgresql.LICENSE.txt"),
            b"license",
        )
        .expect("sidecar");
        assert!(
            installed_drivers(version_dir.path()).is_empty(),
            "a sidecar without the library is not an install",
        );

        fs::write(version_dir.path().join(INSTALLED_LIB), b"ELF").expect("lib");
        assert_eq!(
            installed_drivers(version_dir.path()),
            vec![Driver::Postgresql],
        );
    }

    /// The library name for a platform that is not this host's, so the test
    /// exercises the cross-platform path wherever it runs.
    fn foreign_platform_lib(driver: Driver) -> String {
        let host = Platform::detect().expect("supported host platform");
        let foreign = Platform::ALL
            .iter()
            .copied()
            .find(|platform| *platform != host)
            .expect("another supported platform exists");
        driver.installed_lib_filename(foreign)
    }

    #[test]
    fn installed_drivers_finds_a_library_built_for_another_platform() {
        // `adbc install --platform <other>` leaves a library this host would
        // not look for. list and uninstall must still see it, or the file
        // could never be removed.
        let version_dir = version_dir_with_binaries();
        fs::write(
            version_dir
                .path()
                .join(foreign_platform_lib(Driver::Postgresql)),
            b"FOREIGN",
        )
        .expect("lib");

        assert_eq!(
            installed_drivers(version_dir.path()),
            vec![Driver::Postgresql],
        );
    }

    #[test]
    fn installed_drivers_on_missing_dir_is_empty() {
        let root = tempfile::tempdir().expect("root");
        let missing = root.path().join("nope");

        assert!(installed_drivers(&missing).is_empty());
    }

    #[test]
    fn place_driver_rolls_back_when_a_later_move_fails() {
        let version_dir = version_dir_with_binaries();
        let staging = staged_driver(version_dir.path(), b"ELF");
        // A directory where a sidecar must land makes that rename fail, after
        // the library has already been moved.
        fs::create_dir(
            version_dir
                .path()
                .join("amp-adbc-driver-postgresql.LICENSE.txt"),
        )
        .expect("blocking directory");

        let result = place_driver(
            &staging,
            version_dir.path(),
            Driver::Postgresql,
            Platform::Linux,
        );

        assert!(result.is_err(), "a blocked sidecar move fails the install");
        assert!(
            !version_dir.path().join(INSTALLED_LIB).exists(),
            "the library is rolled back, so list cannot report a failed install",
        );
        assert!(
            installed_drivers(version_dir.path()).is_empty(),
            "no driver is reported after a failed install",
        );
    }
}
