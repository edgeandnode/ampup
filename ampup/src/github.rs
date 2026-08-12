use std::sync::Arc;

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::Deserialize;

use crate::{DEFAULT_REPO, DEFAULT_SELF_REPO, rate_limiter::GitHubRateLimiter};

const AMPUP_API_URL: &str = "https://ampup.sh/api";
const GITHUB_API_URL: &str = "https://api.github.com";
const GITHUB_API_HOST: &str = "api.github.com";

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
/// Produced by [`ReleaseAssets::resolve`] and consumed by
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
    /// Expected content digest from release metadata (e.g. "sha256:<hex>"),
    /// or `None` when the release does not advertise one.
    pub digest: Option<String>,
}

/// The assets of a single fetched release.
///
/// Produced by [`GitHubClient::fetch_release_assets`] so that release metadata
/// is fetched once and individual assets are then resolved in memory via
/// [`ReleaseAssets::resolve`] — no redundant API calls, and each caller pairs a
/// resolved asset with its own request directly instead of relying on the
/// positional alignment of two parallel collections.
pub struct ReleaseAssets {
    repo: String,
    version: String,
    assets: Vec<Asset>,
}

impl ReleaseAssets {
    /// Resolve a single asset by name against this release.
    ///
    /// Returns `Ok(Some(_))` when the asset is present, `Ok(None)` when an
    /// *optional* asset is absent, and `Err(GitHubError::AssetNotFound)` when a
    /// *required* asset (`optional == false`) is missing.
    pub fn resolve(&self, name: &str, optional: bool) -> anyhow::Result<Option<ResolvedAsset>> {
        match self.assets.iter().find(|a| a.name == name) {
            Some(asset) => Ok(Some(ResolvedAsset {
                id: asset.id,
                name: asset.name.clone(),
                url: asset.url.clone(),
                digest: asset.digest.clone(),
            })),
            // Optional artifacts may be missing from a release; skip them.
            None if optional => Ok(None),
            None => Err(GitHubError::AssetNotFound {
                repo: self.repo.clone(),
                asset_name: name.to_string(),
                version: self.version.clone(),
                available_assets: self.assets.iter().map(|a| a.name.clone()).collect(),
            }
            .into()),
        }
    }
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
    #[serde(default)]
    digest: Option<String>,
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
    /// local mock server instead. `github_token` lets a test assert which
    /// hosts the credential is, and is not, sent to.
    #[cfg(test)]
    pub(crate) fn with_api_base(api_base: String, github_token: Option<String>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .build()
            .context("Failed to create request client")?;
        let rate_limiter = Arc::new(GitHubRateLimiter::new(github_token.is_some()));

        Ok(Self {
            client,
            repo: "test/repo".to_string(),
            token: github_token,
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

    /// Fetch a release's asset metadata with a single API call.
    ///
    /// The returned [`ReleaseAssets`] resolves individual assets in memory via
    /// [`ReleaseAssets::resolve`], so callers can fetch once and resolve many
    /// without re-hitting the API and without aligning parallel collections.
    pub async fn fetch_release_assets(&self, version: &str) -> anyhow::Result<ReleaseAssets> {
        let release = self.get_tagged_release(version).await?;
        Ok(ReleaseAssets {
            repo: self.repo.clone(),
            version: version.to_string(),
            assets: release.assets,
        })
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

    /// Attach the configured GitHub credential, but only to requests bound for
    /// the GitHub API.
    ///
    /// The token is resolved from the user's `gh` login, while release metadata
    /// for supported repos is served by `ampup.sh`. Authenticating per-request
    /// rather than through a client-wide default header keeps the credential
    /// from reaching any non-GitHub host.
    fn with_auth(&self, request: reqwest::RequestBuilder, url: &str) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) if is_github_api_url(url) => request.bearer_auth(token),
            _ => request,
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
            .send_with_rate_limit(
                || self.with_auth(self.client.get(&url), &url),
                "Failed to fetch release",
            )
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
                    self.with_auth(
                        self.client
                            .get(&url)
                            .header(reqwest::header::ACCEPT, "application/octet-stream"),
                        &url,
                    )
                },
                "Failed to download asset",
            )
            .await?;

        self.download_response(response, &url, asset_name).await
    }

    /// Download asset directly (for public repos)
    async fn download_asset_direct(&self, url: &str, asset_name: &str) -> Result<Vec<u8>> {
        let response = self
            .send_with_rate_limit(
                || self.with_auth(self.client.get(url), url),
                "Failed to download asset",
            )
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

/// Whether `url` addresses the GitHub API, and so may carry the user's GitHub
/// credential.
///
/// Matches on the parsed host rather than a prefix so a lookalike hostname
/// (`api.github.com.example.net`) cannot claim the token, and requires HTTPS so
/// it is never sent in cleartext.
fn is_github_api_url(url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(url) else {
        return false;
    };

    url.scheme() == "https" && url.host_str() == Some(GITHUB_API_HOST)
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
    use crate::tests::mock_github;

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

    #[tokio::test]
    async fn get_release_with_non_github_api_host_omits_the_github_credential() -> Result<()> {
        //* Given
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let body = mock_github::release_json(addr, "v1.0.0", &[]);
        let (_server, requests) =
            mock_github::start_recording(listener, vec![mock_github::Route::ok("/tags/", body)]);
        let client = GitHubClient::with_api_base(
            format!("http://{addr}"),
            Some("secret-github-token".to_string()),
        )?;

        //* When
        let release = client.get_release("tags/v1.0.0").await?;

        //* Then
        assert_eq!(release.tag, "v1.0.0", "the mock release should be returned");

        let requests = requests
            .lock()
            .expect("recorder lock should not be poisoned");
        assert_eq!(requests.len(), 1, "exactly one request should be recorded");
        assert!(
            !requests[0].to_lowercase().contains("authorization:"),
            "a non-GitHub metadata host must never receive the GitHub credential, got: {}",
            requests[0]
        );

        Ok(())
    }

    #[test]
    fn is_github_api_url_with_github_api_host_returns_true() {
        //* Given
        let url = "https://api.github.com/repos/edgeandnode/amp/releases/assets/1";

        //* When
        let is_github = is_github_api_url(url);

        //* Then
        assert!(
            is_github,
            "the GitHub API host should accept the credential"
        );
    }

    #[test]
    fn is_github_api_url_with_lookalike_host_returns_false() {
        //* Given
        let url = "https://api.github.com.example.net/repos/edgeandnode/amp/releases";

        //* When
        let is_github = is_github_api_url(url);

        //* Then
        assert!(
            !is_github,
            "a host merely prefixed with the GitHub API host must not receive the credential"
        );
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
