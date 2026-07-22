//! A minimal in-process HTTP mock for the GitHub release API, used by
//! integration tests that drive the download path without a network.
//!
//! Bind a listener, describe the routes, and point a
//! [`GitHubClient`](crate::github::GitHubClient) at the returned base URL via
//! `with_api_base`. Requests are matched by a path substring.

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A single mock route: any request whose path contains `prefix` receives
/// `body` with a 200 response.
#[derive(Clone)]
pub(crate) struct Route {
    pub prefix: String,
    pub body: Vec<u8>,
}

impl Route {
    pub fn ok(prefix: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            prefix: prefix.into(),
            body,
        }
    }
}

/// Build the release-metadata JSON GitHub returns for a tag, advertising each
/// asset's download URL (served by the same mock) and optional digest.
pub(crate) fn release_json(
    addr: std::net::SocketAddr,
    tag: &str,
    assets: &[(&str, Option<&str>)],
) -> Vec<u8> {
    let assets: Vec<String> = assets
        .iter()
        .enumerate()
        .map(|(i, (name, digest))| {
            let digest_field = match digest {
                Some(d) => format!(r#","digest":"{d}""#),
                None => String::new(),
            };
            format!(
                r#"{{"id":{},"name":"{}","browser_download_url":"http://{}/download/{}"{}}}"#,
                i + 1,
                name,
                addr,
                name,
                digest_field,
            )
        })
        .collect();
    format!(
        r#"{{"tag_name":"{}","assets":[{}]}}"#,
        tag,
        assets.join(",")
    )
    .into_bytes()
}

/// Spawn the mock server on a pre-bound listener. It reads each request's path,
/// returns the first matching route's body with 200, or 404 if none match.
pub(crate) fn start(
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
                    .find(|r| path.contains(r.prefix.as_str()))
                    .map(|route| {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                            route.body.len()
                        )
                        .into_bytes()
                        .into_iter()
                        .chain(route.body.iter().copied())
                        .collect::<Vec<u8>>()
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
