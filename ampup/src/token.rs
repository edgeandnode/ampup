use std::process::Command;

/// Resolve a GitHub token using the following fallback chain:
///
/// 1. Explicit token passed via `--github-token` flag or `GITHUB_TOKEN` env var
/// 2. Token from `gh auth token` (GitHub CLI)
/// 3. `None` (unauthenticated — lower rate limits)
///
/// Note: `--github-token` values may be visible in process listings (`ps aux`).
/// Prefer `GITHUB_TOKEN` env var or `gh auth token` for sensitive environments.
pub fn resolve_github_token(explicit: Option<String>) -> Option<String> {
    // Filter out empty/whitespace-only tokens so they fall through to the
    // gh CLI fallback instead of sending a useless `Bearer ` header.
    if let Some(token) = explicit
        && !token.trim().is_empty()
    {
        return Some(token);
    }

    try_gh_auth_token()
}

/// Attempt to retrieve a token from the GitHub CLI.
///
/// Runs `gh auth token` as a subprocess. Returns `None` on any failure:
/// `gh` not installed, not logged in, timeout, etc.
fn try_gh_auth_token() -> Option<String> {
    let output = Command::new("gh")
        .args(["auth", "token"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();

    if token.is_empty() {
        return None;
    }

    Some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_github_token_with_explicit_token_returns_explicit() {
        //* Given
        let explicit = Some("my-explicit-token".to_string());

        //* When
        let result = resolve_github_token(explicit);

        //* Then
        assert_eq!(
            result,
            Some("my-explicit-token".to_string()),
            "should return the explicit token without falling through to gh CLI"
        );
    }

    #[test]
    fn resolve_github_token_with_empty_string_falls_through_to_fallback() {
        //* Given
        let explicit = Some("".to_string());

        //* When — empty token should be treated as absent, not as a valid credential
        let result = resolve_github_token(explicit);

        //* Then — result depends on gh CLI availability, but must NOT be Some("")
        assert_ne!(
            result,
            Some("".to_string()),
            "should not return an empty string as a valid token"
        );
    }

    #[test]
    fn resolve_github_token_with_whitespace_only_falls_through_to_fallback() {
        //* Given
        let explicit = Some("   ".to_string());

        //* When
        let result = resolve_github_token(explicit);

        //* Then
        assert_ne!(
            result,
            Some("   ".to_string()),
            "should not return a whitespace-only string as a valid token"
        );
    }
}
