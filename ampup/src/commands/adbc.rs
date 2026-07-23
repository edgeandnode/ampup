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
    version_manager::VersionManager,
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

    // Stage inside the drivers directory so the final rename stays on one
    // filesystem (atomic, no cross-device copy).
    let drivers_dir = version_manager.config().drivers_dir(&version);
    fs::create_dir_all(&drivers_dir).context("failed to create drivers directory")?;
    let staging =
        tempfile::tempdir_in(&drivers_dir).context("failed to create staging directory")?;

    let lib = driver.runtime_lib_filename(platform);
    archive::extract_and_validate(
        &data,
        staging.path(),
        &[lib.as_str(), "LICENSE.txt", "NOTICE.txt"],
    )?;

    // Absolute so the manifest's `Driver.shared` resolves regardless of
    // ampd's working directory.
    let driver_dir = std::path::absolute(
        version_manager
            .config()
            .driver_dir(&version, driver.as_str()),
    )
    .context("failed to resolve driver directory")?;
    place_driver(staging.path(), &driver_dir, &lib)?;
    // The staged files now live at `driver_dir`; disarm TempDir cleanup.
    let _ = staging.keep();

    ui::info!(
        "Installed {} driver for amp {} at {}",
        driver,
        ui::version(&version),
        driver_dir.display(),
    );
    Ok(())
}

/// Assemble the self-contained driver directory: write the manifest into the
/// staged files (pointing `Driver.shared` at the final library path), then
/// atomically move the staged directory into `driver_dir`, replacing any
/// existing install.
fn place_driver(staging: &Path, driver_dir: &Path, lib_name: &str) -> Result<()> {
    let manifest = crate::adbc::driver_manifest(&driver_dir.join(lib_name));
    fs::write(staging.join("manifest.toml"), manifest)
        .context("failed to write ADBC driver manifest")?;

    if driver_dir.exists() {
        fs::remove_dir_all(driver_dir).context("failed to remove the existing driver directory")?;
    }
    fs::rename(staging, driver_dir).context("failed to move the driver into place")?;
    Ok(())
}

/// List installed ADBC drivers for an amp version (the active one by default).
pub fn list(install_dir: Option<PathBuf>, version: Option<String>) -> Result<()> {
    let config = Config::new(install_dir)?;
    let version_manager = VersionManager::new(config);

    // Without an explicit version, having none active is an empty state rather
    // than an error.
    if version.is_none() && version_manager.get_current()?.is_none() {
        ui::info!("No active amp version");
        return Ok(());
    }
    let version = resolve_version(&version_manager, version)?;

    let drivers = installed_drivers(&version_manager.config().drivers_dir(&version))?;
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

    let driver_dir = version_manager
        .config()
        .driver_dir(&version, driver.as_str());
    if !driver_dir.exists() {
        bail!(
            "the {driver} driver is not installed for amp {}",
            ui::version(&version)
        );
    }

    fs::remove_dir_all(&driver_dir).context("failed to remove the driver directory")?;
    // Tidy up an otherwise-empty drivers directory; ignore the error when it
    // still holds other drivers (or a leftover staging dir).
    let _ = fs::remove_dir(version_manager.config().drivers_dir(&version));

    ui::info!(
        "Uninstalled {} driver from amp {}",
        driver,
        ui::version(&version)
    );
    Ok(())
}

/// Resolve the amp version to operate on: an explicit one, or the active one.
///
/// The version must already be installed. Installing amp replaces the whole
/// version directory, so drivers placed under a version whose binaries are not
/// there yet would be destroyed by the next `ampup install`.
fn resolve_version(version_manager: &VersionManager, version: Option<String>) -> Result<String> {
    let version = match version {
        Some(version) => version,
        None => version_manager
            .get_current()?
            .ok_or_else(|| anyhow!("no active amp version; run `ampup install` first"))?,
    };

    if !version_manager.is_installed(&version) {
        bail!(
            "amp {} is not installed; run `ampup install {version}` first",
            ui::version(&version),
        );
    }
    Ok(version)
}

/// The catalog drivers currently installed under `drivers_dir`.
///
/// Only complete installs count: an entry must be a directory named after a
/// known driver and hold a `manifest.toml`, which skips leftover staging
/// directories and partial installs. Returns empty when `drivers_dir` is
/// absent (a version with no drivers installed).
fn installed_drivers(drivers_dir: &Path) -> Result<Vec<Driver>> {
    if !drivers_dir.exists() {
        return Ok(Vec::new());
    }

    let mut drivers = Vec::new();
    for entry in fs::read_dir(drivers_dir).context("failed to read the drivers directory")? {
        let entry = entry.context("failed to read a drivers directory entry")?;
        if !entry
            .file_type()
            .context("failed to determine a drivers entry type")?
            .is_dir()
        {
            continue;
        }
        let Some(driver) = entry.file_name().to_str().and_then(Driver::from_name) else {
            continue;
        };
        if entry.path().join("manifest.toml").is_file() {
            drivers.push(driver);
        }
    }
    drivers.sort_by_key(|d| d.as_str());
    Ok(drivers)
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

    const LIB: &str = "libadbc_driver_postgresql.so";

    /// Build a staging directory holding the three extracted driver files,
    /// with `lib_contents` as the library body.
    fn staged_driver(parent: &Path, lib_contents: &[u8]) -> PathBuf {
        let staging = tempfile::tempdir_in(parent).expect("staging dir");
        fs::write(staging.path().join(LIB), lib_contents).expect("write lib");
        fs::write(staging.path().join("LICENSE.txt"), b"license").expect("write license");
        fs::write(staging.path().join("NOTICE.txt"), b"notice").expect("write notice");
        staging.keep()
    }

    #[test]
    fn place_driver_writes_files_and_manifest() {
        let root = tempfile::tempdir().expect("root");
        let drivers_dir = root.path().join("drivers");
        fs::create_dir_all(&drivers_dir).expect("drivers dir");
        let staging = staged_driver(&drivers_dir, b"ELF");
        let driver_dir = drivers_dir.join("postgresql");

        place_driver(&staging, &driver_dir, LIB).expect("place");

        assert_eq!(fs::read(driver_dir.join(LIB)).expect("lib"), b"ELF");
        assert!(driver_dir.join("LICENSE.txt").exists());
        assert!(driver_dir.join("NOTICE.txt").exists());

        let manifest =
            fs::read_to_string(driver_dir.join("manifest.toml")).expect("manifest exists");
        let parsed: toml::Value = toml::from_str(&manifest).expect("valid TOML");
        assert_eq!(
            parsed["Driver"]["shared"].as_str(),
            Some(driver_dir.join(LIB).to_string_lossy().as_ref()),
        );
    }

    #[test]
    fn place_driver_replaces_an_existing_install() {
        let root = tempfile::tempdir().expect("root");
        let drivers_dir = root.path().join("drivers");
        fs::create_dir_all(&drivers_dir).expect("drivers dir");
        let driver_dir = drivers_dir.join("postgresql");

        place_driver(&staged_driver(&drivers_dir, b"old"), &driver_dir, LIB).expect("first");
        // A stray file from the old install must not survive the reinstall.
        fs::write(driver_dir.join("stale.txt"), b"x").expect("stray file");

        place_driver(&staged_driver(&drivers_dir, b"new"), &driver_dir, LIB).expect("second");

        assert_eq!(fs::read(driver_dir.join(LIB)).expect("lib"), b"new");
        assert!(!driver_dir.join("stale.txt").exists());
    }

    /// Create `drivers_dir/<name>/`, optionally with a `manifest.toml`.
    fn make_driver_dir(drivers_dir: &Path, name: &str, with_manifest: bool) {
        let dir = drivers_dir.join(name);
        fs::create_dir_all(&dir).expect("driver dir");
        if with_manifest {
            fs::write(dir.join("manifest.toml"), b"manifest_version = 1").expect("manifest");
        }
    }

    #[test]
    fn installed_drivers_lists_only_complete_catalog_dirs() {
        let root = tempfile::tempdir().expect("root");
        let drivers_dir = root.path().join("drivers");
        make_driver_dir(&drivers_dir, "postgresql", true); // complete -> included
        make_driver_dir(&drivers_dir, "mysql", true); // not in catalog -> excluded
        make_driver_dir(&drivers_dir, ".tmpABC123", true); // leftover staging -> excluded
        fs::write(drivers_dir.join("stray.txt"), b"x").expect("stray file"); // non-dir -> excluded

        assert_eq!(
            installed_drivers(&drivers_dir).expect("list"),
            vec![Driver::Postgresql],
        );
    }

    #[test]
    fn installed_drivers_skips_partial_installs() {
        let root = tempfile::tempdir().expect("root");
        let drivers_dir = root.path().join("drivers");
        make_driver_dir(&drivers_dir, "postgresql", false); // no manifest -> excluded

        assert!(installed_drivers(&drivers_dir).expect("list").is_empty());
    }

    #[test]
    fn installed_drivers_on_missing_dir_is_empty() {
        let root = tempfile::tempdir().expect("root");
        let drivers_dir = root.path().join("drivers"); // never created

        assert!(installed_drivers(&drivers_dir).expect("list").is_empty());
    }
}
