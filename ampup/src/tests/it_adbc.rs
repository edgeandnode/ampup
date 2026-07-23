//! Integration tests for the `ampup adbc` commands (#2600).
//!
//! `adbc_install_places_driver_and_manifest` drives the whole install path
//! (fetch release metadata -> download -> verify digest -> extract -> place +
//! manifest) against an in-process mock GitHub server, so no network is
//! required. The uninstall test exercises the command against a placed driver.

use flate2::{Compression, write::GzEncoder};
use fs_err as fs;
use sha2::{Digest, Sha256};
use tar::{Builder, Header};

use crate::{
    adbc::Driver,
    commands::adbc,
    config::Config,
    github::GitHubClient,
    tests::{fixtures::TempInstallDir, mock_github},
};

const LIB: &str = "libadbc_driver_postgresql.so";
const ASSET: &str = "adbc-driver-postgresql-linux-x86_64.tar.gz";

/// Build a gzip-compressed tar of `(name, contents)` entries.
fn make_targz(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    for (name, data) in files {
        let mut header = Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, name, *data)
            .expect("should append tar entry");
    }
    builder
        .into_inner()
        .expect("should finish tar")
        .finish()
        .expect("should finish gzip")
}

#[tokio::test]
async fn adbc_install_places_driver_and_manifest() {
    let temp = TempInstallDir::new().expect("temp install dir");
    let version = "v1.0.0";
    fs::write(temp.current_version_file(), version).expect("write active version");

    // The release asset the installer will fetch, plus its advertised digest.
    let tarball = make_targz(&[
        (LIB, b"ELF"),
        ("LICENSE.txt", b"license"),
        ("NOTICE.txt", b"notice"),
    ]);
    let digest = format!("sha256:{:x}", Sha256::digest(&tarball));

    // Mock GitHub: release metadata for the tag, plus the asset download.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server");
    let addr = listener.local_addr().expect("mock server address");
    let release = mock_github::release_json(addr, version, &[(ASSET, Some(&digest))]);
    let routes = vec![
        mock_github::Route::ok(format!("tags/{version}"), release),
        mock_github::Route::ok("download/", tarball),
    ];
    let _server = mock_github::start(listener, routes);

    let github = GitHubClient::with_api_base(format!("http://{addr}")).expect("mock client");
    let config = Config::new(Some(temp.path().to_path_buf())).expect("config");

    adbc::install_driver(
        &github,
        config,
        Driver::Postgresql,
        Some("x86_64".to_string()),
        Some("linux".to_string()),
    )
    .await
    .expect("install should succeed");

    let driver_dir = temp
        .versions_dir()
        .join(version)
        .join("drivers")
        .join("postgresql");
    assert_eq!(fs::read(driver_dir.join(LIB)).expect("lib"), b"ELF");
    assert!(driver_dir.join("LICENSE.txt").exists(), "LICENSE placed");
    assert!(driver_dir.join("NOTICE.txt").exists(), "NOTICE placed");

    let manifest = fs::read_to_string(driver_dir.join("manifest.toml")).expect("manifest exists");
    let parsed: toml::Value = toml::from_str(&manifest).expect("manifest is valid TOML");
    assert_eq!(
        parsed["Driver"]["shared"].as_str(),
        Some(driver_dir.join(LIB).to_string_lossy().as_ref()),
        "Driver.shared points at the placed library",
    );
}

#[tokio::test]
async fn adbc_install_rejects_asset_without_digest() {
    let temp = TempInstallDir::new().expect("temp install dir");
    let version = "v1.0.0";
    fs::write(temp.current_version_file(), version).expect("write active version");

    let tarball = make_targz(&[
        (LIB, b"ELF"),
        ("LICENSE.txt", b"license"),
        ("NOTICE.txt", b"notice"),
    ]);

    // Same release, except the asset advertises no digest.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server");
    let addr = listener.local_addr().expect("mock server address");
    let release = mock_github::release_json(addr, version, &[(ASSET, None)]);
    let routes = vec![
        mock_github::Route::ok(format!("tags/{version}"), release),
        mock_github::Route::ok("download/", tarball),
    ];
    let _server = mock_github::start(listener, routes);

    let github = GitHubClient::with_api_base(format!("http://{addr}")).expect("mock client");
    let config = Config::new(Some(temp.path().to_path_buf())).expect("config");

    let err = adbc::install_driver(
        &github,
        config,
        Driver::Postgresql,
        Some("x86_64".to_string()),
        Some("linux".to_string()),
    )
    .await
    .expect_err("install should refuse an asset without a digest");
    assert!(err.to_string().contains("no digest"), "got: {err}");

    assert!(
        !temp.versions_dir().join(version).join("drivers").exists(),
        "nothing should be installed when the digest is missing",
    );
}

#[test]
fn adbc_uninstall_removes_installed_driver() {
    let temp = TempInstallDir::new().expect("temp install dir");
    let version = "v1.0.0";
    fs::write(temp.current_version_file(), version).expect("write active version");

    let drivers_dir = temp.versions_dir().join(version).join("drivers");
    let driver_dir = drivers_dir.join("postgresql");
    fs::create_dir_all(&driver_dir).expect("driver dir");
    fs::write(driver_dir.join(LIB), b"ELF").expect("lib");
    fs::write(driver_dir.join("manifest.toml"), b"manifest_version = 1").expect("manifest");

    adbc::uninstall(Some(temp.path().to_path_buf()), "postgresql")
        .expect("uninstall should succeed");

    assert!(!driver_dir.exists(), "driver directory removed");
    assert!(
        !drivers_dir.exists(),
        "emptied drivers directory pruned after removing the last driver",
    );
}
