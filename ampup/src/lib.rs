pub mod builder;
pub mod commands;
pub mod config;
pub mod download_manager;
pub mod github;
pub mod install;
pub mod platform;
pub mod progress;
pub mod rate_limiter;
pub mod shell;
pub mod token;
pub mod updater;
pub mod version_manager;

#[macro_use]
pub mod ui;

/// Default GitHub repository for amp releases
pub const DEFAULT_REPO: &str = "edgeandnode/amp";

/// Default number of concurrent downloads
pub const DEFAULT_DOWNLOAD_JOBS: usize = 4;

#[cfg(test)]
mod tests;
