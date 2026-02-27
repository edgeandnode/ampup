use anyhow::Result;

use crate::{
    download_manager::{DownloadManager, DownloadTask},
    platform::{Architecture, Platform},
    progress, ui,
    version_manager::VersionManager,
};

pub struct Installer {
    version_manager: VersionManager,
    download_manager: DownloadManager,
}

impl Installer {
    pub fn new(version_manager: VersionManager, download_manager: DownloadManager) -> Self {
        Self {
            version_manager,
            download_manager,
        }
    }

    /// Install ampd and ampctl from a GitHub release.
    pub async fn install_from_release(
        &self,
        version: &str,
        platform: Platform,
        arch: Architecture,
    ) -> Result<()> {
        self.version_manager.config().ensure_dirs()?;

        let ampd_artifact = format!("ampd-{}-{}", platform.as_str(), arch.as_str());
        let ampctl_artifact = format!("ampctl-{}-{}", platform.as_str(), arch.as_str());

        ui::info!(
            "Downloading {} ({}, {})",
            ui::version(version),
            ampd_artifact,
            ampctl_artifact
        );

        let tasks = vec![
            DownloadTask {
                artifact_name: ampd_artifact,
                dest_filename: "ampd".to_string(),
            },
            DownloadTask {
                artifact_name: ampctl_artifact,
                dest_filename: "ampctl".to_string(),
            },
        ];

        let reporter = progress::create_reporter();
        let version_dir = self.version_manager.config().versions_dir.join(version);

        self.download_manager
            .download_all(tasks, version, version_dir, reporter)
            .await?;

        // Activation barrier: all downloads succeeded, now create symlinks
        self.version_manager.activate(version)?;

        Ok(())
    }
}
