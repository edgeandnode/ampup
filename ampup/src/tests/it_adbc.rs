//! Integration tests for the `ampup adbc` commands (#2600).
//!
//! `adbc_install_places_driver_in_the_version_dir` drives the whole install
//! path (fetch release metadata -> download -> verify digest -> extract ->
//! rename into the version directory) against an in-process mock GitHub
//! server, so no network is required. The uninstall tests exercise the command
//! against a placed driver.

use flate2::{Compression, write::GzEncoder};
use fs_err as fs;
use sha2::{Digest, Sha256};
use tar::{Builder, Header};

use crate::{
    adbc::{DRIVER_FILE_PREFIX, Driver},
    commands::adbc,
    config::Config,
    github::GitHubClient,
    tests::{
        fixtures::{MockBinary, TempInstallDir},
        mock_github,
    },
};

/// The library name inside the release archive, already its final on-disk name
/// (the release pipeline names it, edgeandnode/amp #2599).
const INSTALLED_LIB: &str = "amp-adbc-driver-postgresql.so";
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

/// The driver archive as the release ships it: the library under its final
/// name, plus the Apache-2.0 license files at the root.
fn driver_tarball() -> Vec<u8> {
    make_targz(&[
        (INSTALLED_LIB, b"ELF"),
        ("LICENSE.txt", b"license"),
        ("NOTICE.txt", b"notice"),
    ])
}

/// Names of any driver-owned or leftover staging entries in `version_dir`,
/// ignoring the amp binaries that share it.
///
/// What "no driver installed" means now that drivers are files among others.
fn driver_artifacts(version_dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(version_dir) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(DRIVER_FILE_PREFIX) || name.starts_with(".tmp"))
        .collect();
    found.sort();
    found
}

#[tokio::test]
async fn adbc_install_places_driver_in_the_version_dir() {
    let temp = TempInstallDir::new().expect("temp install dir");
    let version = "v1.0.0";
    fs::write(temp.current_version_file(), version).expect("write active version");
    MockBinary::create(&temp, version).expect("install version binaries");

    let tarball = driver_tarball();
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

    let github = GitHubClient::with_api_base(format!("http://{addr}"), None).expect("mock client");
    let config = Config::new(Some(temp.path().to_path_buf())).expect("config");

    adbc::install_driver(
        &github,
        config,
        Driver::Postgresql,
        Some("x86_64".to_string()),
        Some("linux".to_string()),
        None,
    )
    .await
    .expect("install should succeed");

    // The library lands beside the amp binaries, under its installed name.
    let version_dir = temp.version_dir(version);
    assert_eq!(
        fs::read(version_dir.join(INSTALLED_LIB)).expect("lib"),
        b"ELF",
    );
    // The license files ship in the archive but are not placed on disk.
    assert!(
        !version_dir.join("LICENSE.txt").exists(),
        "the license file is not extracted onto disk",
    );
    assert!(!version_dir.join("NOTICE.txt").exists());

    // The amp binaries it now shares a directory with are untouched.
    for binary in ["ampd", "ampctl", "ampsql"] {
        assert!(version_dir.join(binary).exists(), "{binary} intact");
    }

    // No staging directory is left behind.
    assert!(
        !driver_artifacts(&version_dir)
            .iter()
            .any(|name| name.starts_with(".tmp")),
        "staging directory cleaned up: {:?}",
        driver_artifacts(&version_dir),
    );
}

#[tokio::test]
async fn adbc_install_rejects_asset_without_digest() {
    let temp = TempInstallDir::new().expect("temp install dir");
    let version = "v1.0.0";
    fs::write(temp.current_version_file(), version).expect("write active version");
    MockBinary::create(&temp, version).expect("install version binaries");

    // Same release, except the asset advertises no digest.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server");
    let addr = listener.local_addr().expect("mock server address");
    let release = mock_github::release_json(addr, version, &[(ASSET, None)]);
    let routes = vec![
        mock_github::Route::ok(format!("tags/{version}"), release),
        mock_github::Route::ok("download/", driver_tarball()),
    ];
    let _server = mock_github::start(listener, routes);

    let github = GitHubClient::with_api_base(format!("http://{addr}"), None).expect("mock client");
    let config = Config::new(Some(temp.path().to_path_buf())).expect("config");

    let err = adbc::install_driver(
        &github,
        config,
        Driver::Postgresql,
        Some("x86_64".to_string()),
        Some("linux".to_string()),
        None,
    )
    .await
    .expect_err("install should refuse an asset without a digest");
    assert!(err.to_string().contains("no digest"), "got: {err}");

    assert!(
        driver_artifacts(&temp.version_dir(version)).is_empty(),
        "nothing installed and no staging left behind when the digest is missing",
    );
}

#[tokio::test]
async fn adbc_install_refuses_a_version_that_is_not_installed() {
    let temp = TempInstallDir::new().expect("temp install dir");
    // v1.0.0 is active and installed; v2.0.0 has no binaries.
    fs::write(temp.current_version_file(), "v1.0.0").expect("write active version");
    MockBinary::create(&temp, "v1.0.0").expect("install version binaries");

    // No mock server: resolving the version fails before anything is fetched.
    let github =
        GitHubClient::with_api_base("http://127.0.0.1:1".to_string(), None).expect("client");
    let config = Config::new(Some(temp.path().to_path_buf())).expect("config");

    let err = adbc::install_driver(
        &github,
        config,
        Driver::Postgresql,
        Some("x86_64".to_string()),
        Some("linux".to_string()),
        Some("v2.0.0".to_string()),
    )
    .await
    .expect_err("install should refuse a version that is not installed");
    assert!(err.to_string().contains("not installed"), "got: {err}");

    // Installing amp replaces the whole version directory, so drivers must not
    // be placed under a version whose binaries are not there yet.
    assert!(
        !temp.versions_dir().join("v2.0.0").exists(),
        "no version directory should be created for an uninstalled version",
    );
}

#[tokio::test]
async fn adbc_install_targets_an_explicit_version() {
    let temp = TempInstallDir::new().expect("temp install dir");
    // Active version differs from the one being targeted.
    fs::write(temp.current_version_file(), "v1.0.0").expect("write active version");
    MockBinary::create(&temp, "v1.0.0").expect("install active version binaries");
    let target = "v2.0.0";
    MockBinary::create(&temp, target).expect("install target version binaries");

    let tarball = driver_tarball();
    let digest = format!("sha256:{:x}", Sha256::digest(&tarball));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server");
    let addr = listener.local_addr().expect("mock server address");
    let release = mock_github::release_json(addr, target, &[(ASSET, Some(&digest))]);
    let routes = vec![
        mock_github::Route::ok(format!("tags/{target}"), release),
        mock_github::Route::ok("download/", tarball),
    ];
    let _server = mock_github::start(listener, routes);

    let github = GitHubClient::with_api_base(format!("http://{addr}"), None).expect("mock client");
    let config = Config::new(Some(temp.path().to_path_buf())).expect("config");

    adbc::install_driver(
        &github,
        config,
        Driver::Postgresql,
        Some("x86_64".to_string()),
        Some("linux".to_string()),
        Some(target.to_string()),
    )
    .await
    .expect("install should succeed for an explicit version");

    assert!(
        temp.version_dir(target).join(INSTALLED_LIB).exists(),
        "driver installed under the requested version",
    );
    assert!(
        driver_artifacts(&temp.version_dir("v1.0.0")).is_empty(),
        "the active version holds no driver files",
    );
}

#[test]
fn adbc_uninstall_removes_the_library() {
    let temp = TempInstallDir::new().expect("temp install dir");
    let version = "v1.0.0";
    fs::write(temp.current_version_file(), version).expect("write active version");
    MockBinary::create(&temp, version).expect("install version binaries");

    let version_dir = temp.version_dir(version);
    fs::write(version_dir.join(INSTALLED_LIB), b"ELF").expect("lib");

    adbc::uninstall(Some(temp.path().to_path_buf()), "postgresql", None)
        .expect("uninstall should succeed");

    assert!(
        driver_artifacts(&version_dir).is_empty(),
        "the driver library is removed",
    );
    // Uninstalling a driver must not disturb the amp binaries it sat beside.
    for binary in ["ampd", "ampctl", "ampsql"] {
        assert!(version_dir.join(binary).exists(), "{binary} intact");
    }
}

#[test]
fn adbc_uninstall_works_for_a_version_without_binaries() {
    let temp = TempInstallDir::new().expect("temp install dir");
    let version = "v1.0.0";
    fs::write(temp.current_version_file(), version).expect("write active version");

    // An orphaned driver: binaries are gone, the library remains. Cleanup must
    // still work, otherwise it could never be removed with ampup.
    let version_dir = temp.version_dir(version);
    fs::create_dir_all(&version_dir).expect("version dir");
    fs::write(version_dir.join(INSTALLED_LIB), b"ELF").expect("lib");

    adbc::uninstall(Some(temp.path().to_path_buf()), "postgresql", None)
        .expect("uninstall should work without the version's binaries");

    assert!(!version_dir.join(INSTALLED_LIB).exists(), "library removed");
}

#[test]
fn adbc_uninstall_removes_a_library_built_for_another_platform() {
    let temp = TempInstallDir::new().expect("temp install dir");
    let version = "v1.0.0";
    fs::write(temp.current_version_file(), version).expect("write active version");
    MockBinary::create(&temp, version).expect("install version binaries");

    // `--platform <other>` leaves a library this host would not look for.
    // Uninstall must still find it, or it could never be removed.
    let host = crate::platform::Platform::detect().expect("supported host platform");
    let other = crate::platform::Platform::ALL
        .iter()
        .copied()
        .find(|platform| *platform != host)
        .expect("another supported platform exists");
    let version_dir = temp.version_dir(version);
    let foreign = version_dir.join(Driver::Postgresql.installed_lib_filename(other));
    fs::write(&foreign, b"FOREIGN").expect("lib");

    adbc::uninstall(Some(temp.path().to_path_buf()), "postgresql", None)
        .expect("uninstall should remove a foreign-platform library");

    assert!(!foreign.exists(), "foreign-platform library removed");
}
