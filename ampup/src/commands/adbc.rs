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
    download_manager::verify_artifact,
    github::GitHubClient,
    platform::{Architecture, Platform, PlatformError},
    token, ui,
    version_manager::VersionManager,
};

/// Install an ADBC driver for the active amp version.
pub async fn install(
    driver: &str,
    install_dir: Option<PathBuf>,
    repo: String,
    github_token: Option<String>,
    arch: Option<String>,
    platform: Option<String>,
) -> Result<()> {
    let driver = parse_driver(driver)?;

    let config = Config::new(install_dir)?;
    let token = token::resolve_github_token(github_token);
    let github = GitHubClient::new(repo, token)?;
    let version_manager = VersionManager::new(config);

    // Drivers are pinned to an installed amp version, so one must be active.
    let version = version_manager
        .get_current()?
        .ok_or_else(|| anyhow!("no active amp version; run `ampup install` first"))?;

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

    let data = github.download_resolved_asset(&asset).await?;
    verify_artifact(&asset.name, &data, asset.digest.as_deref())
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
}
