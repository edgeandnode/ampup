use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use fs_err as fs;
use tokio::{sync::Semaphore, task::JoinSet};

use crate::github::{GitHubClient, ResolvedAsset};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single artifact to download from a GitHub release.
pub struct DownloadTask {
    /// GitHub release asset name (e.g., "ampd-linux-x86_64")
    pub artifact_name: String,
    /// Destination filename inside the version directory (e.g., "ampd")
    pub dest_filename: String,
}

/// Errors that occur during bounded-concurrent download operations.
///
/// Used by [`DownloadManager::download_all`] and its helper functions.
/// Each variant represents a distinct failure mode in the download-verify-stage
/// pipeline.
#[derive(Debug)]
pub enum DownloadError {
    /// A download task failed after one automatic retry.
    ///
    /// The download was attempted twice (initial + one retry) and both attempts
    /// failed. The `source` error contains the retry failure with the initial
    /// failure chained as context.
    ///
    /// Possible causes:
    /// - Network connectivity issues (DNS, timeout, connection reset)
    /// - GitHub API errors (5xx, asset removed mid-download)
    /// - Rate limiting that persisted through the HTTP-layer retry
    TaskFailed {
        artifact_name: String,
        source: anyhow::Error,
    },

    /// Downloaded artifact was empty (zero bytes).
    ///
    /// The HTTP request succeeded but the response body contained no data.
    /// This typically indicates a problem with the release packaging on GitHub
    /// rather than a network issue.
    EmptyArtifact { artifact_name: String },

    /// Failed to write an artifact to the staging directory.
    ///
    /// The download and verification succeeded, but writing the artifact data
    /// to the temporary staging directory failed. This usually indicates a
    /// filesystem issue (permissions, disk full, path too long).
    StagingWrite {
        artifact_name: String,
        path: PathBuf,
        source: std::io::Error,
    },

    /// Internal error: the concurrency semaphore was unexpectedly closed.
    ///
    /// This should not occur under normal operation. It indicates a logic bug
    /// where the semaphore was dropped or closed while tasks were still waiting
    /// to acquire permits.
    SemaphoreClosed { artifact_name: String },
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TaskFailed {
                artifact_name,
                source,
            } => {
                writeln!(f, "Download failed for artifact")?;
                writeln!(f, "  Artifact: {}", artifact_name)?;
                writeln!(f, "  Error: {}", source)?;
                writeln!(f)?;
                writeln!(f, "  The download was retried once and still failed.")?;
                write!(f, "  Check your network connection and try again.")?;
            }
            Self::EmptyArtifact { artifact_name } => {
                writeln!(f, "Downloaded artifact is empty")?;
                writeln!(f, "  Artifact: {}", artifact_name)?;
                writeln!(f)?;
                writeln!(
                    f,
                    "  The release asset was downloaded but contains no data."
                )?;
                write!(
                    f,
                    "  This may indicate a problem with the release packaging."
                )?;
            }
            Self::StagingWrite {
                artifact_name,
                path,
                source,
            } => {
                writeln!(f, "Failed to write artifact to staging directory")?;
                writeln!(f, "  Artifact: {}", artifact_name)?;
                writeln!(f, "  Path: {}", path.display())?;
                write!(f, "  Error: {}", source)?;
            }
            Self::SemaphoreClosed { artifact_name } => {
                writeln!(f, "Internal error: concurrency semaphore closed")?;
                write!(f, "  Artifact: {}", artifact_name)?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for DownloadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::TaskFailed { source, .. } => Some(source.as_ref()),
            Self::StagingWrite { source, .. } => Some(source),
            Self::EmptyArtifact { .. } | Self::SemaphoreClosed { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// DownloadManager
// ---------------------------------------------------------------------------

/// Manages bounded-concurrent downloads of release artifacts.
///
/// Downloads proceed in parallel up to `max_concurrent` tasks. Each task
/// downloads an artifact, verifies it (currently: non-empty check), and
/// writes it to a staging directory. Only after all tasks succeed does
/// the staging directory get atomically renamed to the final version
/// directory.
///
/// If any task fails (after one retry), all in-flight tasks are cancelled
/// and the staging directory is cleaned up automatically via `TempDir` drop.
pub struct DownloadManager {
    github: GitHubClient,
    max_concurrent: usize,
}

impl DownloadManager {
    /// Create a new download manager.
    ///
    /// `max_concurrent` is clamped to a minimum of 1 to prevent deadlocks.
    /// Pass 1 for sequential downloads (useful for debugging).
    pub fn new(github: GitHubClient, max_concurrent: usize) -> Self {
        Self {
            github,
            max_concurrent: max_concurrent.max(1),
        }
    }

    /// Download all tasks concurrently and write results to `version_dir`.
    ///
    /// Fetches release metadata once, then spawns bounded-concurrent download
    /// tasks. Uses a staging directory (sibling of `version_dir`) for
    /// atomicity. If all downloads succeed and pass verification, the staging
    /// directory is renamed to `version_dir`. If any download fails, all
    /// in-flight tasks are cancelled and the staging directory is cleaned up.
    pub async fn download_all(
        &self,
        tasks: Vec<DownloadTask>,
        version: &str,
        version_dir: PathBuf,
    ) -> Result<()> {
        // Resolve all asset metadata with a single API call so that each
        // spawned task can download directly without re-fetching the release.
        let asset_names: Vec<&str> = tasks.iter().map(|t| t.artifact_name.as_str()).collect();
        let resolved = self
            .github
            .resolve_release_assets(version, &asset_names)
            .await?;

        let parent = version_dir.parent().ok_or_else(|| {
            anyhow::anyhow!("version_dir has no parent: {}", version_dir.display())
        })?;

        // Staging dir in the same parent ensures same filesystem for atomic rename
        let staging_dir =
            tempfile::tempdir_in(parent).context("Failed to create staging directory")?;

        let semaphore = Arc::new(Semaphore::new(self.max_concurrent));
        let mut join_set: JoinSet<std::result::Result<(), DownloadError>> = JoinSet::new();

        for (task, asset) in tasks.into_iter().zip(resolved) {
            let github = self.github.clone();
            let sem = semaphore.clone();
            let staging_path = staging_dir.path().to_path_buf();

            join_set.spawn(async move {
                let _permit = sem
                    .acquire()
                    .await
                    .map_err(|_| DownloadError::SemaphoreClosed {
                        artifact_name: task.artifact_name.clone(),
                    })?;

                let data = download_with_retry(&github, &asset).await?;
                verify_artifact(&task.artifact_name, &data)?;
                write_to_staging(&staging_path, &task.dest_filename, &data)?;

                Ok(())
            });
        }

        // Collect results — fail fast on first error
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    join_set.shutdown().await;
                    return Err(e.into());
                }
                Err(join_err) => {
                    join_set.shutdown().await;
                    return Err(anyhow::anyhow!("download task panicked: {}", join_err));
                }
            }
        }

        // Set executable permissions on all staged files
        #[cfg(unix)]
        set_executable_permissions(staging_dir.path())?;

        // Safe replacement: rename existing dir to backup, swap staging in, then
        // remove backup. If the swap fails, restore from backup so the previous
        // install is never lost.
        //
        // NOTE: `with_extension("old")` is wrong for semver names because it
        // replaces the last dotted segment (v0.1.0 → v0.1.old), causing
        // collisions across patch versions. Instead, append ".old" to the full
        // directory name.
        let backup_dir = append_extension(&version_dir, "old");
        // Remove stale backup from a previous crashed install so the backup
        // rename below doesn't fail with ENOTEMPTY.
        if backup_dir.exists() {
            let _ = fs::remove_dir_all(&backup_dir);
        }
        if version_dir.exists() {
            fs::rename(&version_dir, &backup_dir).with_context(|| {
                format!(
                    "failed to back up existing version directory {}",
                    version_dir.display()
                )
            })?;
        }

        // Atomic move: staging → version_dir.
        // `keep()` detaches the TempDir so `drop` won't remove it. If the
        // rename below fails we must clean up `staging_path` ourselves.
        let staging_path = staging_dir.keep();
        if let Err(err) = fs::rename(&staging_path, &version_dir) {
            // Clean up the orphaned staging directory
            let _ = fs::remove_dir_all(&staging_path);

            // Restore backup so the previous install is not lost
            if backup_dir.exists()
                && let Err(restore_err) = fs::rename(&backup_dir, &version_dir)
            {
                crate::ui::warn!(
                    "Failed to restore previous install from {}: {}",
                    backup_dir.display(),
                    restore_err
                );
            }
            return Err(err).with_context(|| {
                format!(
                    "failed to move staging directory to {}",
                    version_dir.display()
                )
            });
        }

        // Best-effort cleanup of the backup
        if backup_dir.exists() {
            let _ = fs::remove_dir_all(&backup_dir);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Append an extension to a path without replacing an existing dotted segment.
///
/// Unlike [`Path::with_extension`], which replaces the last dotted segment
/// (e.g. `v0.1.0` → `v0.1.old`), this function appends to the full name
/// (e.g. `v0.1.0` → `v0.1.0.old`). This is critical for semver-style
/// directory names where dot-separated components are not file extensions.
fn append_extension(path: &Path, ext: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

/// Download a resolved asset with one retry on failure.
///
/// This targets network and transient errors. Rate-limit retries (429) are
/// already handled at the HTTP layer by `GitHubClient::send_with_rate_limit`,
/// so a rate-limited request will have been retried there before surfacing
/// as an error here.
async fn download_with_retry(
    github: &GitHubClient,
    asset: &ResolvedAsset,
) -> std::result::Result<Vec<u8>, DownloadError> {
    // `false` suppresses per-file progress bars — DownloadManager will
    // provide aggregate progress reporting in a future PR.
    match github.download_resolved_asset(asset, false).await {
        Ok(data) => Ok(data),
        Err(first_err) => {
            crate::ui::warn!("Download failed for {}, retrying once...", asset.name);

            github
                .download_resolved_asset(asset, false)
                .await
                .map_err(|retry_err| DownloadError::TaskFailed {
                    artifact_name: asset.name.clone(),
                    source: retry_err
                        .context(format!("retry also failed (first error: {})", first_err)),
                })
        }
    }
}

/// Verify a downloaded artifact. Currently checks non-empty.
/// Per-artifact checksum/attestation verification will be added in a follow-up PR.
fn verify_artifact(artifact_name: &str, data: &[u8]) -> std::result::Result<(), DownloadError> {
    if data.is_empty() {
        return Err(DownloadError::EmptyArtifact {
            artifact_name: artifact_name.to_string(),
        });
    }
    Ok(())
}

/// Write artifact data to the staging directory.
fn write_to_staging(
    staging_path: &Path,
    dest_filename: &str,
    data: &[u8],
) -> std::result::Result<(), DownloadError> {
    let dest = staging_path.join(dest_filename);
    fs::write(&dest, data).map_err(|err| DownloadError::StagingWrite {
        artifact_name: dest_filename.to_string(),
        path: dest,
        source: err,
    })
}

/// Set executable permissions (0o755) on all files in a directory.
#[cfg(unix)]
fn set_executable_permissions(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    for entry in fs::read_dir(dir).context("failed to list staging directory")? {
        let entry = entry.context("failed to read staging directory entry")?;
        let path = entry.path();
        let mut perms = fs::metadata(&path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms)
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    mod verify_artifact {
        use super::*;

        /// Rejects zero-byte downloads so corrupt installs are caught early.
        #[test]
        fn verify_artifact_with_empty_data_returns_empty_artifact_error() {
            //* Given
            let data: Vec<u8> = vec![];

            //* When
            let result = verify_artifact("ampd-linux-x86_64", &data);

            //* Then
            let err = result.expect_err("should return DownloadError for empty data");
            assert!(
                matches!(err, DownloadError::EmptyArtifact { .. }),
                "expected EmptyArtifact error, got: {:?}",
                err
            );
        }
    }

    #[cfg(unix)]
    mod set_executable_permissions {
        use std::os::unix::fs::PermissionsExt;

        use super::*;

        /// Ensures every file in the version directory is marked executable.
        #[test]
        fn set_executable_permissions_with_multiple_files_sets_0o755_on_all() {
            //* Given
            let dir = tempfile::tempdir().expect("should create temp directory");
            fs::write(dir.path().join("ampd"), b"ampd-binary").expect("should write ampd");
            fs::write(dir.path().join("ampctl"), b"ampctl-binary").expect("should write ampctl");

            //* When
            let result = set_executable_permissions(dir.path());

            //* Then
            assert!(result.is_ok(), "should set permissions on all files");
            for name in ["ampd", "ampctl"] {
                let perms = fs::metadata(dir.path().join(name))
                    .expect("should read metadata")
                    .permissions();
                assert_eq!(
                    perms.mode() & 0o777,
                    0o755,
                    "{} should have 0o755 permissions",
                    name
                );
            }
        }
    }

    /// Tests the backup-swap-cleanup pattern used by `download_all` to replace
    /// an existing version directory safely. These exercise the filesystem
    /// operations directly (not through `download_all`) because the full method
    /// requires a live `GitHubClient`.
    mod staging_swap {
        use super::*;

        /// Verifies that an existing version directory is atomically replaced
        /// with fresh content via the backup-swap pattern.
        #[test]
        fn backup_swap_with_existing_version_dir_replaces_contents() {
            //* Given
            let parent = tempfile::tempdir().expect("should create parent directory");
            let version_dir = parent.path().join("v1.0.0");
            let backup_dir = append_extension(&version_dir, "old");
            fs::create_dir_all(&version_dir).expect("should create stale version directory");
            fs::write(version_dir.join("stale-file"), b"stale").expect("should write stale file");

            let staging_dir =
                tempfile::tempdir_in(parent.path()).expect("should create staging directory");
            fs::write(staging_dir.path().join("ampd"), b"fresh-binary")
                .expect("should write to staging");

            //* When — mirrors the backup-swap logic in download_all
            let backup_result = fs::rename(&version_dir, &backup_dir);
            let staging_path = staging_dir.keep();
            let swap_result = fs::rename(&staging_path, &version_dir);

            //* Then
            assert!(backup_result.is_ok(), "should back up existing version_dir");
            assert!(swap_result.is_ok(), "should swap staging into version_dir");
            let content = fs::read(version_dir.join("ampd")).expect("should read installed binary");
            assert_eq!(
                content, b"fresh-binary",
                "version_dir should contain the fresh binary, not stale data"
            );

            // Cleanup backup
            if backup_dir.exists() {
                let _ = fs::remove_dir_all(&backup_dir);
            }
        }

        /// Handles a leftover `.old` directory from a previously crashed install.
        #[test]
        fn backup_swap_with_stale_backup_dir_succeeds() {
            //* Given — a leftover .old dir from a previous crashed install
            let parent = tempfile::tempdir().expect("should create parent directory");
            let version_dir = parent.path().join("v1.0.0");
            let backup_dir = append_extension(&version_dir, "old");
            fs::create_dir_all(&version_dir).expect("should create version directory");
            fs::write(version_dir.join("ampd"), b"current").expect("should write current binary");
            fs::create_dir_all(&backup_dir).expect("should create stale backup directory");
            fs::write(backup_dir.join("ampd"), b"stale-backup").expect("should write stale backup");

            //* When — mirrors the stale backup cleanup + backup-swap logic
            if backup_dir.exists() {
                let _ = fs::remove_dir_all(&backup_dir);
            }
            let backup_result = fs::rename(&version_dir, &backup_dir);

            //* Then
            assert!(
                backup_result.is_ok(),
                "should succeed after removing stale backup directory"
            );
            assert!(
                !version_dir.exists(),
                "version_dir should be moved to backup"
            );
            assert!(
                backup_dir.exists(),
                "backup_dir should contain the previous install"
            );

            // Cleanup
            let _ = fs::remove_dir_all(&backup_dir);
        }
    }

    mod append_extension {
        use super::*;

        /// Guards against `with_extension` truncating semver names like `v0.1.0`.
        #[test]
        fn append_extension_with_semver_name_preserves_full_name() {
            //* Given
            let path = PathBuf::from("/versions/v0.1.0");

            //* When
            let result = append_extension(&path, "old");

            //* Then
            assert_eq!(
                result,
                PathBuf::from("/versions/v0.1.0.old"),
                "should append .old to the full directory name, not replace .0"
            );
        }
    }

    /// End-to-end tests for `download_all` using a mock HTTP server.
    /// Exercises bounded concurrency, fail-fast cancellation, retry
    /// behavior, and the staging-to-version-dir swap.
    mod it_download_all {
        use std::sync::atomic::{AtomicUsize, Ordering};

        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        use super::*;

        /// Route configuration for the mock HTTP server.
        #[derive(Clone)]
        struct Route {
            /// Path substring to match against the request path.
            prefix: &'static str,
            /// Response body to return on success.
            body: Vec<u8>,
            /// Number of times to return 500 before succeeding.
            /// Shared across connections so retries see the updated count.
            fail_count: Arc<AtomicUsize>,
        }

        impl Route {
            /// Create a route that always succeeds.
            fn ok(prefix: &'static str, body: Vec<u8>) -> Self {
                Self {
                    prefix,
                    body,
                    fail_count: Arc::new(AtomicUsize::new(0)),
                }
            }

            /// Create a route that returns 500 for the first `n` requests,
            /// then succeeds.
            fn fail_then_ok(prefix: &'static str, body: Vec<u8>, n: usize) -> Self {
                Self {
                    prefix,
                    body,
                    fail_count: Arc::new(AtomicUsize::new(n)),
                }
            }
        }

        /// Spawn a mock HTTP server on a pre-bound listener.
        ///
        /// `routes` maps path substrings to response bodies. The server accepts
        /// connections in a loop, reads the request line to extract the path,
        /// and sends back the matching response with 200 OK — or 404 if no
        /// route matches. Routes with a non-zero `fail_count` return 500 until
        /// the count is exhausted.
        fn start_mock_server(
            listener: tokio::net::TcpListener,
            routes: Vec<Route>,
        ) -> tokio::task::JoinHandle<()> {
            tokio::spawn(async move {
                loop {
                    let Ok((mut stream, _)) = listener.accept().await else {
                        break;
                    };
                    let routes = routes.clone();

                    tokio::spawn(async move {
                        let mut buf = [0u8; 4096];
                        let n = stream.read(&mut buf).await.expect("should read request");
                        let request = String::from_utf8_lossy(&buf[..n]);
                        let path = request
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1))
                            .unwrap_or("/");

                        let response = routes
                            .iter()
                            .find(|r| path.contains(r.prefix))
                            .map(|route| {
                                // Atomically decrement the fail counter. Uses
                                // compare_exchange in a loop to avoid the race
                                // where fetch_sub wraps 0 → usize::MAX.
                                let should_fail = loop {
                                    let current = route.fail_count.load(Ordering::Relaxed);
                                    if current == 0 {
                                        break false;
                                    }
                                    if route
                                        .fail_count
                                        .compare_exchange(
                                            current,
                                            current - 1,
                                            Ordering::Relaxed,
                                            Ordering::Relaxed,
                                        )
                                        .is_ok()
                                    {
                                        break true;
                                    }
                                };

                                if should_fail {
                                    b"HTTP/1.1 500 Internal Server Error\r\n\
                                      Content-Length: 0\r\n\r\n"
                                        .to_vec()
                                } else {
                                    format!(
                                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                                        route.body.len()
                                    )
                                    .into_bytes()
                                    .into_iter()
                                    .chain(route.body.iter().copied())
                                    .collect::<Vec<u8>>()
                                }
                            })
                            .unwrap_or_else(|| {
                                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
                            });

                        stream
                            .write_all(&response)
                            .await
                            .expect("should write response");
                    });
                }
            })
        }

        /// Build release JSON for the mock server with the given assets.
        fn release_json(addr: std::net::SocketAddr, asset_names: &[&str]) -> Vec<u8> {
            let assets: Vec<String> = asset_names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    format!(
                        r#"{{"id":{},"name":"{}","browser_download_url":"http://{}/download/{}"}}"#,
                        i + 1,
                        name,
                        addr,
                        name,
                    )
                })
                .collect();
            format!(r#"{{"tag_name":"v1.0.0","assets":[{}]}}"#, assets.join(",")).into_bytes()
        }

        /// Common test setup: bind a mock server, create a `DownloadManager`,
        /// and prepare a temp directory with a version path.
        struct TestFixture {
            manager: DownloadManager,
            version_dir: PathBuf,
            _tmp: tempfile::TempDir,
            server_handle: tokio::task::JoinHandle<()>,
        }

        impl TestFixture {
            /// Create a fixture with the given release assets, download routes,
            /// and concurrency level. The release metadata route is prepended
            /// automatically.
            async fn new(
                release_assets: &[&str],
                download_routes: Vec<Route>,
                max_concurrent: usize,
            ) -> Self {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("should bind to a random port");
                let addr = listener.local_addr().expect("should have a local address");

                let release_body = release_json(addr, release_assets);
                let mut routes = vec![Route::ok("tags/v1.0.0", release_body)];
                routes.extend(download_routes);

                let server_handle = start_mock_server(listener, routes);

                let api_base = format!("http://{}", addr);
                let github =
                    GitHubClient::with_api_base(api_base).expect("should create test client");
                let manager = DownloadManager::new(github, max_concurrent);

                let tmp = tempfile::tempdir().expect("should create temp directory");
                let version_dir = tmp.path().join("v1.0.0");

                Self {
                    manager,
                    version_dir,
                    _tmp: tmp,
                    server_handle,
                }
            }

            /// Run `download_all` with the given tasks.
            async fn download(&self, tasks: Vec<DownloadTask>) -> Result<()> {
                self.manager
                    .download_all(tasks, "v1.0.0", self.version_dir.clone())
                    .await
            }
        }

        impl Drop for TestFixture {
            fn drop(&mut self) {
                self.server_handle.abort();
            }
        }

        /// The standard two-artifact task list (ampd + ampctl).
        fn standard_tasks() -> Vec<DownloadTask> {
            vec![
                DownloadTask {
                    artifact_name: "ampd-linux-x86_64".to_string(),
                    dest_filename: "ampd".to_string(),
                },
                DownloadTask {
                    artifact_name: "ampctl-linux-x86_64".to_string(),
                    dest_filename: "ampctl".to_string(),
                },
            ]
        }

        /// Happy path: both artifacts download and land in the version directory.
        #[tokio::test]
        async fn download_all_with_two_assets_writes_both_to_version_dir() {
            //* Given
            let ampd_data = b"fake-ampd-binary".to_vec();
            let ampctl_data = b"fake-ampctl-binary".to_vec();

            let fixture = TestFixture::new(
                &["ampd-linux-x86_64", "ampctl-linux-x86_64"],
                vec![
                    Route::ok("download/ampd-linux-x86_64", ampd_data.clone()),
                    Route::ok("download/ampctl-linux-x86_64", ampctl_data.clone()),
                ],
                4,
            )
            .await;

            //* When
            let result = fixture.download(standard_tasks()).await;

            //* Then
            assert!(
                result.is_ok(),
                "download_all should succeed: {:?}",
                result.err()
            );
            assert_eq!(
                fs::read(fixture.version_dir.join("ampd")).expect("should read ampd"),
                ampd_data,
                "ampd binary should match downloaded content"
            );
            assert_eq!(
                fs::read(fixture.version_dir.join("ampctl")).expect("should read ampctl"),
                ampctl_data,
                "ampctl binary should match downloaded content"
            );
        }

        /// A missing asset fails the whole batch and leaves no partial install.
        #[tokio::test]
        async fn download_all_with_missing_asset_fails_without_partial_install() {
            //* Given — release only contains ampd; ampctl is missing
            let fixture = TestFixture::new(
                &["ampd-linux-x86_64"],
                vec![Route::ok(
                    "download/ampd-linux-x86_64",
                    b"fake-ampd-binary".to_vec(),
                )],
                4,
            )
            .await;

            //* When
            let result = fixture.download(standard_tasks()).await;

            //* Then
            assert!(
                result.is_err(),
                "download_all should fail when an asset is missing from the release"
            );
            assert!(
                !fixture.version_dir.exists(),
                "version_dir should not exist after failed download (no partial install)"
            );
        }

        /// `-j 1` (sequential) mode still produces a correct install.
        #[tokio::test]
        async fn download_all_with_sequential_mode_succeeds() {
            //* Given — same as happy path but with max_concurrent = 1
            let fixture = TestFixture::new(
                &["ampd-linux-x86_64", "ampctl-linux-x86_64"],
                vec![
                    Route::ok("download/ampd-linux-x86_64", b"ampd-bytes".to_vec()),
                    Route::ok("download/ampctl-linux-x86_64", b"ampctl-bytes".to_vec()),
                ],
                1,
            )
            .await;

            //* When
            let result = fixture.download(standard_tasks()).await;

            //* Then
            assert!(
                result.is_ok(),
                "download_all with -j 1 should succeed: {:?}",
                result.err()
            );
            assert!(
                fixture.version_dir.join("ampd").exists(),
                "ampd should be installed in version_dir"
            );
            assert!(
                fixture.version_dir.join("ampctl").exists(),
                "ampctl should be installed in version_dir"
            );
        }

        /// A single 500 is retried and the download ultimately succeeds.
        #[tokio::test]
        async fn download_all_with_transient_failure_succeeds_on_retry() {
            //* Given — ampd download returns 500 on the first request, then 200
            let ampd_data = b"ampd-after-retry".to_vec();

            let fixture = TestFixture::new(
                &["ampd-linux-x86_64"],
                vec![Route::fail_then_ok(
                    "download/ampd-linux-x86_64",
                    ampd_data.clone(),
                    1, // fail once, then succeed
                )],
                4,
            )
            .await;

            let tasks = vec![DownloadTask {
                artifact_name: "ampd-linux-x86_64".to_string(),
                dest_filename: "ampd".to_string(),
            }];

            //* When
            let result = fixture.download(tasks).await;

            //* Then
            assert!(
                result.is_ok(),
                "download_all should succeed after retry: {:?}",
                result.err()
            );
            assert_eq!(
                fs::read(fixture.version_dir.join("ampd")).expect("should read ampd"),
                ampd_data,
                "ampd binary should contain data from the successful retry"
            );
        }

        /// Persistent 500s exhaust all retries and fail with no partial install.
        #[tokio::test]
        async fn download_all_with_persistent_failure_fails_after_retry() {
            //* Given — ampd download returns 500 on all attempts.
            //  Each download attempt triggers 2 HTTP requests (initial + 5xx
            //  retry in send_with_rate_limit), and download_with_retry makes 2
            //  attempts, so 4 failures are needed for persistent failure.
            let fixture = TestFixture::new(
                &["ampd-linux-x86_64"],
                vec![Route::fail_then_ok(
                    "download/ampd-linux-x86_64",
                    b"should-never-be-read".to_vec(),
                    4, // 2 attempts × 2 HTTP requests each (initial + 5xx retry)
                )],
                4,
            )
            .await;

            let tasks = vec![DownloadTask {
                artifact_name: "ampd-linux-x86_64".to_string(),
                dest_filename: "ampd".to_string(),
            }];

            //* When
            let result = fixture.download(tasks).await;

            //* Then
            assert!(
                result.is_err(),
                "download_all should fail when both initial and retry fail"
            );
            assert!(
                !fixture.version_dir.exists(),
                "version_dir should not exist after permanent failure"
            );
        }
    }
}
