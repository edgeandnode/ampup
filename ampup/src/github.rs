use std::sync::Arc;

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::Deserialize;

use crate::{DEFAULT_REPO, DEFAULT_SELF_REPO, rate_limiter::GitHubRateLimiter};

const AMPUP_API_URL: &str = "https://ampup.sh/api";
const GITHUB_API_URL: &str = "https://api.github.com";

#[derive(Debug)]
pub enum GitHubError {
    ReleaseNotFound {
        repo: String,
        has_token: bool,
        url: String,
        is_latest: bool,
    },
    AuthFailed {
        status_code: u16,
        repo: String,
        url: String,
    },
    AssetNotFound {
        repo: String,
        asset_name: String,
        version: String,
        available_assets: Vec<String>,
    },
    DownloadFailed {
        repo: String,
        asset_name: String,
        status_code: u16,
        url: String,
    },
    HttpError {
        repo: String,
        status_code: u16,
        url: String,
        body: String,
    },
    RateLimited {
        retry_after_secs: u64,
        has_token: bool,
    },
}

impl std::fmt::Display for GitHubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReleaseNotFound {
                repo,
                has_token,
                url,
                is_latest,
            } => {
                if *is_latest {
                    writeln!(f, "Failed to fetch latest release")?;
                } else {
                    writeln!(f, "Failed to fetch release")?;
                }
                writeln!(f, "  Repository: {}", repo)?;
                writeln!(f, "  URL: {}", url)?;
                writeln!(f, "  Status: 404 Not Found")?;
                writeln!(f)?;
                if *has_token {
                    writeln!(
                        f,
                        "  The repository may not exist, or no releases have been published yet."
                    )?;
                    if !*is_latest {
                        writeln!(f, "  The specified version/tag may not exist.")?;
                    }
                } else {
                    writeln!(f, "  The repository is private or requires authentication.")?;
                    writeln!(f, "  Try: export GITHUB_TOKEN=$(gh auth token)")?;
                }
            }
            Self::AuthFailed {
                status_code,
                repo,
                url,
            } => {
                writeln!(f, "Authentication failed")?;
                writeln!(f, "  Repository: {}", repo)?;
                writeln!(f, "  URL: {}", url)?;
                writeln!(f, "  Status: HTTP {}", status_code)?;
                writeln!(f)?;
                writeln!(f, "  Your GITHUB_TOKEN may be invalid or expired.")?;
                if *status_code == 403 {
                    writeln!(
                        f,
                        "  For private repositories, ensure your token has 'repo' scope."
                    )?;
                }
                writeln!(f, "  Try: export GITHUB_TOKEN=$(gh auth token)")?;
            }
            Self::AssetNotFound {
                repo,
                asset_name,
                version,
                available_assets,
            } => {
                writeln!(f, "Release asset not found")?;
                writeln!(f, "  Repository: {}", repo)?;
                writeln!(f, "  Asset: {}", asset_name)?;
                writeln!(f, "  Version: {}", version)?;
                writeln!(f)?;
                if available_assets.is_empty() {
                    writeln!(f, "  No assets available in this release.")?;
                } else {
                    writeln!(f, "  Available assets:")?;
                    for asset in available_assets {
                        writeln!(f, "    - {}", asset)?;
                    }
                }
            }
            Self::DownloadFailed {
                repo,
                asset_name,
                status_code,
                url,
            } => {
                writeln!(f, "Failed to download release asset")?;
                writeln!(f, "  Repository: {}", repo)?;
                writeln!(f, "  Asset: {}", asset_name)?;
                writeln!(f, "  URL: {}", url)?;
                writeln!(f, "  Status: HTTP {}", status_code)?;
                writeln!(f)?;
                if *status_code == 401 || *status_code == 403 {
                    writeln!(f, "  Authentication or permission issue.")?;
                    writeln!(f, "  Try: export GITHUB_TOKEN=$(gh auth token)")?;
                } else if *status_code == 404 {
                    writeln!(f, "  The asset may have been removed or is not accessible.")?;
                } else {
                    writeln!(f, "  Network or server error. Please try again.")?;
                }
            }
            Self::HttpError {
                repo,
                status_code,
                url,
                body,
            } => {
                writeln!(f, "Request failed")?;
                writeln!(f, "  Repository: {}", repo)?;
                writeln!(f, "  URL: {}", url)?;
                writeln!(f, "  Status: HTTP {}", status_code)?;
                if !body.is_empty() {
                    writeln!(f, "  Response: {}", body)?;
                }
            }
            Self::RateLimited {
                retry_after_secs,
                has_token,
            } => {
                writeln!(f, "GitHub API rate limit exceeded")?;
                writeln!(f, "  Retry after: {} seconds", retry_after_secs)?;
                writeln!(f)?;
                if !*has_token {
                    writeln!(f, "  Unauthenticated requests have lower rate limits.")?;
                    writeln!(f, "  Try: export GITHUB_TOKEN=$(gh auth token)")?;
                }
            }
        }
        Ok(())
    }
}

impl std::error::Error for GitHubError {}

/// A release asset resolved from GitHub metadata, ready to download.
///
/// Produced by [`GitHubClient::resolve_release_assets`] and consumed by
/// [`GitHubClient::download_resolved_asset`]. This allows fetching release
/// metadata once and then downloading multiple assets without redundant API
/// calls.
#[derive(Clone, Debug)]
pub struct ResolvedAsset {
    /// Asset ID on GitHub (used for API-based downloads of private repos).
    pub id: u64,
    /// Asset name (e.g. "ampd-linux-x86_64").
    pub name: String,
    /// Direct browser download URL (used for public repos).
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct Release {
    #[serde(rename = "tag_name")]
    tag: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    id: u64,
    name: String,
    #[serde(rename = "browser_download_url")]
    url: String,
}

/// Cloneable so `DownloadManager` can move a handle into each spawned task.
/// `reqwest::Client` and `rate_limiter` are `Arc`-backed; `repo` and `token`
/// are small strings cloned by value.
#[derive(Clone)]
pub struct GitHubClient {
    client: reqwest::Client,
    repo: String,
    token: Option<String>,
    /// Base URL for API requests (either custom API or GitHub API)
    api: String,
    rate_limiter: Arc<GitHubRateLimiter>,
}

impl GitHubClient {
    pub fn new(repo: String, github_token: Option<String>) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("ampup"),
        );

        if let Some(token) = &github_token {
            let auth_value = format!("Bearer {}", token);
            headers.insert(
                reqwest::header::AUTHORIZATION,
                reqwest::header::HeaderValue::from_str(&auth_value)
                    .context("Invalid access token")?,
            );
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to create request client")?;

        let api = release_api_base(&repo);

        let rate_limiter = Arc::new(GitHubRateLimiter::new(github_token.is_some()));

        Ok(Self {
            client,
            repo,
            token: github_token,
            api,
            rate_limiter,
        })
    }

    /// Create a client with a custom API base URL for testing.
    ///
    /// `api_base` replaces the standard GitHub API URL so requests go to a
    /// local mock server instead.
    #[cfg(test)]
    pub(crate) fn with_api_base(api_base: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .build()
            .context("Failed to create request client")?;
        let rate_limiter = Arc::new(GitHubRateLimiter::new(false));

        Ok(Self {
            client,
            repo: "test/repo".to_string(),
            token: None,
            api: api_base,
            rate_limiter,
        })
    }

    /// Get the latest release version
    pub async fn get_latest_version(&self) -> Result<String> {
        let release = self.get_latest_release().await?;
        Ok(release.tag)
    }

    /// Get the latest release
    async fn get_latest_release(&self) -> Result<Release> {
        self.get_release("latest").await
    }

    /// Get a tagged release
    async fn get_tagged_release(&self, version: &str) -> Result<Release> {
        self.get_release(&format!("tags/{}", version)).await
    }

    /// Wait for any active rate-limit pause, or fail if the wait is too long.
    async fn check_rate_limit_pause(&self) -> Result<()> {
        if let Err(duration) = self.rate_limiter.wait_if_paused().await {
            return Err(GitHubError::RateLimited {
                retry_after_secs: duration.as_secs(),
                has_token: self.token.is_some(),
            }
            .into());
        }
        Ok(())
    }

    /// Find an asset by name within a release, returning `AssetNotFound` if
    /// no match exists.
    fn find_asset<'a>(
        &self,
        release: &'a Release,
        asset_name: &str,
        version: &str,
    ) -> Result<&'a Asset> {
        release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| {
                GitHubError::AssetNotFound {
                    repo: self.repo.clone(),
                    asset_name: asset_name.to_string(),
                    version: version.to_string(),
                    available_assets: release.assets.iter().map(|a| a.name.clone()).collect(),
                }
                .into()
            })
    }

    /// Resolve multiple asset names from a single release, fetching the release
    /// metadata only once.
    ///
    /// Returns a `ResolvedAsset` for each requested name. Fails with
    /// `GitHubError::AssetNotFound` on the first name that does not match any
    /// asset in the release.
    pub async fn resolve_release_assets(
        &self,
        version: &str,
        asset_names: &[&str],
    ) -> Result<Vec<ResolvedAsset>> {
        let release = self.get_tagged_release(version).await?;

        let mut resolved = Vec::with_capacity(asset_names.len());
        for &name in asset_names {
            let asset = self.find_asset(&release, name, version)?;
            resolved.push(ResolvedAsset {
                id: asset.id,
                name: asset.name.clone(),
                url: asset.url.clone(),
            });
        }
        Ok(resolved)
    }

    /// Download a previously resolved asset without re-fetching release
    /// metadata.
    pub async fn download_resolved_asset(&self, asset: &ResolvedAsset) -> Result<Vec<u8>> {
        if self.token.is_some() {
            self.download_asset_via_api(asset.id, &asset.name).await
        } else {
            self.download_asset_direct(&asset.url, &asset.name).await
        }
    }

    /// Send a request with rate-limit awareness, one retry on 429, and one
    /// retry on transient server/transport errors.
    ///
    /// Retry order:
    /// 1. Rate-limit (429/403-rate-limited) — wait for `Retry-After`, retry once
    /// 2. Server error (5xx) — 1-second delay, retry once
    /// 3. Transport error (connection reset, DNS, timeout) — 1-second delay, retry once
    ///
    /// These retries protect metadata fetches (`get_release`,
    /// `resolve_release_assets`) which have no outer retry layer. Download
    /// paths have an additional retry in `DownloadManager::download_with_retry`.
    async fn send_with_rate_limit(
        &self,
        build_request: impl Fn() -> reqwest::RequestBuilder,
        context_msg: &str,
    ) -> Result<reqwest::Response> {
        self.check_rate_limit_pause().await?;

        let response = match build_request().send().await {
            Ok(resp) => resp,
            Err(first_err) => {
                // One retry on transport errors (connection reset, DNS, timeout)
                crate::ui::warn!("Request failed ({}), retrying once...", first_err);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                self.check_rate_limit_pause().await?;

                build_request().send().await.with_context(|| {
                    format!(
                        "{} (retry also failed, first error: {})",
                        context_msg, first_err
                    )
                })?
            }
        };

        let response =
            if let Some(retry_after) = self.rate_limiter.update_from_response(&response).await {
                crate::ui::warn!(
                    "Rate limited by GitHub API, retrying in {} seconds...",
                    retry_after
                );
                self.check_rate_limit_pause().await?;

                let response = build_request()
                    .send()
                    .await
                    .with_context(|| context_msg.to_string())?;

                if let Some(retry_after) = self.rate_limiter.update_from_response(&response).await {
                    return Err(GitHubError::RateLimited {
                        retry_after_secs: retry_after,
                        has_token: self.token.is_some(),
                    }
                    .into());
                }

                response
            } else {
                response
            };

        // One retry on server errors (5xx) — transient GitHub/CDN blips
        if response.status().is_server_error() {
            crate::ui::warn!(
                "Server error (HTTP {}), retrying once...",
                response.status().as_u16()
            );
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            self.check_rate_limit_pause().await?;

            let response = build_request()
                .send()
                .await
                .with_context(|| context_msg.to_string())?;

            self.rate_limiter.update_from_response(&response).await;
            return Ok(response);
        }

        // Warn if rate limit is exhausted (preemptive pause applies to next request)
        if self.rate_limiter.remaining().await == Some(0) {
            crate::ui::warn!(
                "GitHub API rate limit exhausted, subsequent requests will be paused until reset"
            );
        }

        Ok(response)
    }

    /// Fetch release from GitHub API
    async fn get_release(&self, path: &str) -> Result<Release> {
        let url = format!("{}/{}", self.api, path);

        let response = self
            .send_with_rate_limit(|| self.client.get(&url), "Failed to fetch release")
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            match status {
                reqwest::StatusCode::NOT_FOUND => {
                    return Err(GitHubError::ReleaseNotFound {
                        repo: self.repo.clone(),
                        has_token: self.token.is_some(),
                        url: url.clone(),
                        is_latest: path == "latest",
                    }
                    .into());
                }
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                    return Err(GitHubError::AuthFailed {
                        status_code: status.as_u16(),
                        repo: self.repo.clone(),
                        url: url.clone(),
                    }
                    .into());
                }
                _ => {
                    let body = response.text().await.unwrap_or_default();
                    return Err(GitHubError::HttpError {
                        repo: self.repo.clone(),
                        status_code: status.as_u16(),
                        url: url.clone(),
                        body,
                    }
                    .into());
                }
            }
        }

        let release: Release = response
            .json()
            .await
            .context("Failed to parse release response")?;

        Ok(release)
    }

    /// Download a release asset by name.
    pub async fn download_release_asset(&self, version: &str, asset_name: &str) -> Result<Vec<u8>> {
        let release = self.get_tagged_release(version).await?;
        let asset = self.find_asset(&release, asset_name, version)?;

        if self.token.is_some() {
            // For private repositories, we need to use the API to download
            self.download_asset_via_api(asset.id, asset_name).await
        } else {
            // For public repositories, use direct download URL
            self.download_asset_direct(&asset.url, asset_name).await
        }
    }

    /// Download asset via GitHub API (for private repos)
    async fn download_asset_via_api(&self, asset_id: u64, asset_name: &str) -> Result<Vec<u8>> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/assets/{}",
            self.repo, asset_id
        );

        let response = self
            .send_with_rate_limit(
                || {
                    self.client
                        .get(&url)
                        .header(reqwest::header::ACCEPT, "application/octet-stream")
                },
                "Failed to download asset",
            )
            .await?;

        self.download_response(response, &url, asset_name).await
    }

    /// Download asset directly (for public repos)
    async fn download_asset_direct(&self, url: &str, asset_name: &str) -> Result<Vec<u8>> {
        let response = self
            .send_with_rate_limit(|| self.client.get(url), "Failed to download asset")
            .await?;

        self.download_response(response, url, asset_name).await
    }

    /// Stream a response body into a buffer.
    async fn download_response(
        &self,
        response: reqwest::Response,
        url: &str,
        asset_name: &str,
    ) -> Result<Vec<u8>> {
        if !response.status().is_success() {
            let status = response.status();
            return Err(GitHubError::DownloadFailed {
                repo: self.repo.clone(),
                asset_name: asset_name.to_string(),
                status_code: status.as_u16(),
                url: url.to_string(),
            }
            .into());
        }

        // Stream and collect chunks
        let mut buffer = Vec::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Error while downloading file")?;
            buffer.extend_from_slice(&chunk);
        }

        Ok(buffer)
    }
}

fn release_api_base(repo: &str) -> String {
    match repo_slug(repo) {
        Some(slug) => format!("{}/{}", AMPUP_API_URL, slug),
        None => format!("{}/repos/{}/releases", GITHUB_API_URL, repo),
    }
}

fn repo_slug(repo: &str) -> Option<&'static str> {
    match repo {
        DEFAULT_REPO => Some("amp"),
        DEFAULT_SELF_REPO => Some("ampup"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;

    #[test]
    fn release_api_base_with_amp_repo_uses_ampup_api_slug() {
        //* When
        let api_base = release_api_base(DEFAULT_REPO);

        //* Then
        assert_eq!(
            api_base, "https://ampup.sh/api/amp",
            "amp releases should use the ampup API amp slug"
        );
    }

    #[test]
    fn release_api_base_with_ampup_repo_uses_ampup_api_slug() {
        //* When
        let api_base = release_api_base(DEFAULT_SELF_REPO);

        //* Then
        assert_eq!(
            api_base, "https://ampup.sh/api/ampup",
            "ampup releases should use the ampup API ampup slug"
        );
    }

    #[test]
    fn new_with_ampup_repo_and_github_token_uses_ampup_api() -> Result<()> {
        //* Given
        let github_token = Some("test-token".to_string());

        //* When
        let client = GitHubClient::new(DEFAULT_SELF_REPO.to_string(), github_token)?;

        //* Then
        assert_eq!(
            client.api, "https://ampup.sh/api/ampup",
            "supported repos should use ampup.sh API even when a GitHub token is configured"
        );

        Ok(())
    }

    #[test]
    fn release_api_base_with_other_repo_uses_github_releases_api() {
        //* Given
        let repo = "some-owner/some-repo";

        //* When
        let api_base = release_api_base(repo);

        //* Then
        assert_eq!(
            api_base, "https://api.github.com/repos/some-owner/some-repo/releases",
            "unsupported repos should keep using the GitHub releases API"
        );
    }
}
