//! Optional ADBC driver components (#2600).
//!
//! Installs destination-specific ADBC driver libraries from the pinned set
//! shipped with each Amp release, so `ampd` can load them at runtime.

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

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
    _jobs: usize,
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

    // Extract into a staging dir; placement into the version's drivers
    // directory is handled separately.
    let staging = tempfile::tempdir().context("failed to create staging directory")?;
    let lib = driver.runtime_lib_filename(platform);
    archive::extract_and_validate(
        &data,
        staging.path(),
        &[lib.as_str(), "LICENSE.txt", "NOTICE.txt"],
    )?;

    ui::detail!(
        "Fetched, verified, and extracted the {} driver ({} bytes)",
        driver,
        data.len()
    );
    bail!("installing the driver into the amp version is not yet implemented");
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
