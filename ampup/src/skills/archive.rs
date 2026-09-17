//! Reading selected files out of a GitHub repository source tarball.
//!
//! Entry paths that are not plain relative paths (`..`, absolute paths) are
//! dropped, so nothing read from a tarball can resolve outside the repository.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufReader, Read as _, Seek as _, SeekFrom},
    path::{Component, Path},
};

/// A regular file read from a repository source tarball.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveFile {
    /// The file's bytes.
    pub contents: Vec<u8>,
    /// Whether any execute bit is set, so scripts stay runnable.
    pub is_executable: bool,
}

/// Which files to read from a source tarball.
///
/// A repository tarball holds the whole repository, so only the needed files
/// are kept in memory.
pub enum Selection<'a> {
    /// Every file under this directory, for example `"skills/"`.
    UnderDir(&'a str),
    /// Exactly these repository paths.
    Paths(&'a BTreeSet<String>),
}

impl Selection<'_> {
    fn contains(&self, repo_path: &str) -> bool {
        match self {
            Self::UnderDir(dir) => repo_path.starts_with(dir),
            Self::Paths(paths) => paths.contains(repo_path),
        }
    }
}

/// Read the selected regular files from a GitHub source tarball (`.tar.gz`).
///
/// GitHub wraps the repository in a single top-level directory named after the
/// repository and commit (e.g. `owner-repo-0123abc/`). That directory is
/// stripped, so the returned map is keyed by repository path (e.g.
/// `skills/amp-query/SKILL.md`). Directories, symlinks, and other entry types
/// are skipped.
///
/// Reads from the start of `tarball` every time, so the same file can be read
/// once per selection.
///
/// # Errors
///
/// Fails when the file cannot be rewound, the tarball is not valid gzip or tar,
/// or an entry's header or contents cannot be read. See [`ReadFilesError`].
pub fn read_files(
    tarball: &File,
    selection: Selection,
) -> Result<BTreeMap<String, ArchiveFile>, ReadFilesError> {
    // `&File` implements `Read` and `Seek`, so a shared reference is enough to
    // rewind the file and read it again
    let mut reader = tarball;
    reader
        .seek(SeekFrom::Start(0))
        .map_err(ReadFilesError::Rewind)?;

    let decoder = flate2::read::GzDecoder::new(BufReader::new(reader));
    let mut archive = tar::Archive::new(decoder);

    let mut files = BTreeMap::new();

    let entries = archive.entries().map_err(ReadFilesError::Entries)?;
    for entry in entries {
        let mut entry = entry.map_err(ReadFilesError::Entry)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let entry_path = entry
            .path()
            .map_err(ReadFilesError::EntryPath)?
            .into_owned();
        let Some(repo_path) = repo_path_of(&entry_path) else {
            continue;
        };
        if !selection.contains(&repo_path) {
            continue;
        }

        let mode = match entry.header().mode() {
            Ok(mode) => mode,
            Err(err) => {
                return Err(ReadFilesError::EntryMode {
                    path: repo_path,
                    source: err,
                });
            }
        };

        let mut contents = Vec::new();
        if let Err(err) = entry.read_to_end(&mut contents) {
            return Err(ReadFilesError::EntryContents {
                path: repo_path,
                source: err,
            });
        }

        files.insert(
            repo_path,
            ArchiveFile {
                contents,
                is_executable: mode & 0o111 != 0,
            },
        );
    }

    Ok(files)
}

/// Errors from reading files out of a source tarball
#[derive(Debug, thiserror::Error)]
pub enum ReadFilesError {
    /// The tarball file could not be rewound to its start
    #[error("Failed to rewind source tarball file")]
    Rewind(#[source] std::io::Error),

    /// The tarball's entry list could not be opened
    ///
    /// Occurs when the download is not a gzip-compressed tar archive.
    #[error("Failed to read source tarball")]
    Entries(#[source] std::io::Error),

    /// An entry header could not be read
    ///
    /// Possible causes:
    /// - The download was truncated
    /// - The archive is corrupted
    #[error("Failed to read source tarball entry")]
    Entry(#[source] std::io::Error),

    /// An entry's path could not be decoded
    #[error("Source tarball entry has an invalid path")]
    EntryPath(#[source] std::io::Error),

    /// A selected entry's file mode could not be parsed from its header
    #[error("Source tarball entry {path} has an invalid mode")]
    EntryMode {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// A selected entry's contents could not be read
    ///
    /// Occurs when the archive ends before the size its header declares.
    #[error("Failed to read {path} from source tarball")]
    EntryContents {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Turn a tarball entry path into a repository path by dropping the top-level
/// directory.
///
/// Returns `None` for entries outside that directory, and for paths that are
/// not plain relative paths (such as `..` or absolute paths), so a malformed
/// tarball can never make ampup write outside the skills directory.
fn repo_path_of(entry_path: &Path) -> Option<String> {
    let mut components = entry_path.components();

    // GitHub names the top-level directory after the repository and commit, so
    // its name carries nothing to check
    let Some(Component::Normal(_)) = components.next() else {
        return None;
    };

    let mut parts = Vec::new();
    for component in components {
        let Component::Normal(part) = component else {
            return None;
        };
        parts.push(part.to_str()?);
    }

    if parts.is_empty() {
        return None;
    }

    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_path_of_with_top_level_dir_strips_it() {
        //* Given
        let entry_path = Path::new("owner-repo-0123abc/skills/amp-query/SKILL.md");

        //* When
        let repo_path = repo_path_of(entry_path);

        //* Then
        assert_eq!(
            repo_path.as_deref(),
            Some("skills/amp-query/SKILL.md"),
            "the top-level tarball directory should be stripped"
        );
    }

    #[test]
    fn repo_path_of_with_parent_dir_component_returns_none() {
        //* Given
        let entry_path = Path::new("owner-repo-0123abc/skills/../../etc/passwd");

        //* When
        let repo_path = repo_path_of(entry_path);

        //* Then
        assert_eq!(
            repo_path, None,
            "paths escaping the repository should be ignored"
        );
    }
}
