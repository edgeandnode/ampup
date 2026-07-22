//! Extraction of ADBC driver release archives (flat gzip-compressed tar).
//!
//! Driver archives contain exactly a driver library plus `LICENSE.txt` and
//! `NOTICE.txt` at the root. Extraction enforces that flat shape and an exact
//! member set, refusing anything that could escape the destination directory.

use std::{
    collections::BTreeSet,
    path::{Component, Path},
};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use tar::Archive;

/// Extract a gzip-compressed tar archive into `dest`, requiring a flat layout
/// whose entries exactly equal `expected_members`.
///
/// Every entry must be a single normal path component (no directories, no
/// `..`), so an entry can never be written outside `dest`. Any unexpected or
/// missing member is an error.
pub fn extract_and_validate(data: &[u8], dest: &Path, expected_members: &[&str]) -> Result<()> {
    let mut archive = Archive::new(GzDecoder::new(data));
    let mut seen = BTreeSet::new();

    for entry in archive
        .entries()
        .context("failed to read archive entries")?
    {
        let mut entry = entry.context("failed to read an archive entry")?;
        let path = entry.path().context("archive entry has an invalid path")?;

        // A flat file name is exactly one normal component. This rejects
        // nested paths, absolute paths, and `..`, so `dest.join(name)` stays
        // inside `dest`.
        let mut components = path.components();
        let name = match (components.next(), components.next()) {
            (Some(Component::Normal(name)), None) => name
                .to_str()
                .context("archive entry name is not valid UTF-8")?
                .to_owned(),
            _ => bail!("archive entry {:?} is not a flat file name", path),
        };

        if !expected_members.contains(&name.as_str()) {
            bail!("unexpected member in driver archive: {name}");
        }

        // Only regular files are expected. A symlink/hardlink entry (even under
        // an allowed name) would be materialized by `unpack` as a link rather
        // than the archived bytes, so reject anything that isn't a plain file.
        if !entry.header().entry_type().is_file() {
            bail!("archive member {name} is not a regular file");
        }

        entry
            .unpack(dest.join(&name))
            .with_context(|| format!("failed to extract {name}"))?;
        seen.insert(name);
    }

    let expected: BTreeSet<String> = expected_members.iter().map(|m| m.to_string()).collect();
    if seen != expected {
        let missing: Vec<String> = expected.difference(&seen).cloned().collect();
        bail!(
            "driver archive is missing expected members: {}",
            missing.join(", ")
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use flate2::{Compression, write::GzEncoder};
    use tar::{Builder, Header};

    use super::*;

    /// Build a gzip-compressed tar archive from `(name, contents)` pairs.
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

    const MEMBERS: &[&str] = &["libadbc_driver_postgresql.so", "LICENSE.txt", "NOTICE.txt"];

    #[test]
    fn extracts_exact_members() {
        let archive = make_targz(&[
            ("libadbc_driver_postgresql.so", b"ELF"),
            ("LICENSE.txt", b"license"),
            ("NOTICE.txt", b"notice"),
        ]);
        let dir = tempfile::tempdir().expect("tempdir");

        extract_and_validate(&archive, dir.path(), MEMBERS).expect("should extract");

        assert_eq!(
            std::fs::read(dir.path().join("libadbc_driver_postgresql.so")).expect("lib"),
            b"ELF",
        );
    }

    #[test]
    fn rejects_unexpected_member() {
        let archive = make_targz(&[("evil.sh", b"rm -rf /")]);
        let dir = tempfile::tempdir().expect("tempdir");

        let err = extract_and_validate(&archive, dir.path(), MEMBERS)
            .expect_err("unexpected member should fail");
        assert!(err.to_string().contains("unexpected member"), "got: {err}");
    }

    #[test]
    fn rejects_missing_member() {
        let archive = make_targz(&[("LICENSE.txt", b"license")]);
        let dir = tempfile::tempdir().expect("tempdir");

        let err = extract_and_validate(&archive, dir.path(), &["LICENSE.txt", "NOTICE.txt"])
            .expect_err("missing member should fail");
        assert!(
            err.to_string().contains("missing expected members"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_non_flat_entry() {
        let archive = make_targz(&[("nested/lib.so", b"x")]);
        let dir = tempfile::tempdir().expect("tempdir");

        let err = extract_and_validate(&archive, dir.path(), &["lib.so"])
            .expect_err("nested entry should fail");
        assert!(err.to_string().contains("flat file name"), "got: {err}");
    }

    #[test]
    fn rejects_symlink_entry() {
        // A symlink named as an allowed member, pointing outside the archive.
        let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        let mut header = Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o644);
        builder
            .append_link(&mut header, "libadbc_driver_postgresql.so", "/etc/passwd")
            .expect("should append symlink entry");
        let archive = builder
            .into_inner()
            .expect("should finish tar")
            .finish()
            .expect("should finish gzip");
        let dir = tempfile::tempdir().expect("tempdir");

        let err = extract_and_validate(&archive, dir.path(), MEMBERS)
            .expect_err("symlink member should fail");
        assert!(err.to_string().contains("not a regular file"), "got: {err}");
    }
}
