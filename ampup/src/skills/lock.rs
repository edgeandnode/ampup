//! The lock file recording which skills ampup installed in a skills directory.
//!
//! Skills missing from the lock file were not installed by ampup, and ampup
//! never updates or removes them.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use fs_err as fs;

use super::SkillName;

/// Name of the file that records which skills ampup installed in a directory.
pub const LOCK_FILE_NAME: &str = ".amp-skills.json";

/// The skills ampup installed in one skills directory.
///
/// Stored as [`LOCK_FILE_NAME`] inside that directory. Skills that are not
/// listed here were not installed by ampup, and ampup never updates or removes
/// them.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SkillsLock {
    /// Installed skills, keyed by skill name.
    ///
    /// Keys are validated on load, so a tampered lock file cannot name a path
    /// outside the skills directory.
    pub skills: BTreeMap<SkillName, InstalledSkill>,
}

/// Where an installed skill was downloaded from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstalledSkill {
    /// GitHub repository in format `owner/repo`.
    pub repo: String,
    /// The ref the user asked for (tag, branch, or commit).
    #[serde(rename = "ref")]
    pub git_ref: String,
    /// The commit the ref pointed to at install time.
    pub commit: String,
}

impl SkillsLock {
    /// Load the lock file from a skills directory.
    ///
    /// Returns an empty lock when the file does not exist.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read, is not valid JSON, or names a skill
    /// that is not a valid [`SkillName`]. See [`LoadSkillsLockError`].
    pub fn load(skills_dir: &Path) -> Result<Self, LoadSkillsLockError> {
        let lock_path = skills_dir.join(LOCK_FILE_NAME);
        if !lock_path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&lock_path).map_err(LoadSkillsLockError::Read)?;
        serde_json::from_str(&contents).map_err(|err| LoadSkillsLockError::Parse {
            path: lock_path,
            source: err,
        })
    }

    /// Save the lock file to a skills directory.
    ///
    /// Deletes the file instead when no skills are left.
    ///
    /// # Errors
    ///
    /// Fails when the skills directory or the file cannot be written, or the
    /// file cannot be deleted. See [`SaveSkillsLockError`].
    pub fn save(&self, skills_dir: &Path) -> Result<(), SaveSkillsLockError> {
        let lock_path = skills_dir.join(LOCK_FILE_NAME);

        if self.skills.is_empty() {
            if lock_path.exists() {
                fs::remove_file(&lock_path).map_err(SaveSkillsLockError::Remove)?;
            }
            return Ok(());
        }

        let mut contents =
            serde_json::to_string_pretty(self).map_err(SaveSkillsLockError::Serialize)?;
        contents.push('\n');

        fs::create_dir_all(skills_dir).map_err(SaveSkillsLockError::CreateSkillsDir)?;
        fs::write(&lock_path, contents).map_err(SaveSkillsLockError::Write)?;

        Ok(())
    }
}

/// Errors from loading a skills lock file
#[derive(Debug, thiserror::Error)]
pub enum LoadSkillsLockError {
    /// The lock file exists but could not be read
    ///
    /// Possible causes:
    /// - Permission denied
    /// - The path is a directory
    #[error("Failed to read skills lock file")]
    Read(#[source] std::io::Error),

    /// The lock file is not valid JSON, or names an invalid skill
    ///
    /// Occurs when the file was edited by hand, including a skill name that is
    /// `..` or contains a path separator. Nothing is removed or overwritten
    /// when this happens.
    #[error("Invalid skills lock file {}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Errors from saving a skills lock file
#[derive(Debug, thiserror::Error)]
pub enum SaveSkillsLockError {
    /// The lock could not be serialized to JSON
    #[error("Failed to serialize skills lock file")]
    Serialize(#[source] serde_json::Error),

    /// The skills directory holding the lock file could not be created
    #[error("Failed to create skills directory")]
    CreateSkillsDir(#[source] std::io::Error),

    /// The lock file could not be written
    #[error("Failed to write skills lock file")]
    Write(#[source] std::io::Error),

    /// The empty lock file could not be deleted
    #[error("Failed to remove skills lock file")]
    Remove(#[source] std::io::Error),
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn load_with_saved_lock_returns_same_skills() -> Result<()> {
        //* Given
        let skills_dir = TempDir::new()?;
        let mut lock = SkillsLock::default();
        lock.skills.insert(
            "amp-query".parse()?,
            InstalledSkill {
                repo: "edgeandnode/amp".to_string(),
                git_ref: "v0.1.0".to_string(),
                commit: "0123456789abcdef".to_string(),
            },
        );
        lock.save(skills_dir.path())?;

        //* When
        let loaded = SkillsLock::load(skills_dir.path())?;

        //* Then
        assert_eq!(loaded, lock, "loaded lock should match the saved lock");

        Ok(())
    }

    #[test]
    fn save_with_no_skills_removes_lock_file() -> Result<()> {
        //* Given
        let skills_dir = TempDir::new()?;
        let lock_path = skills_dir.path().join(LOCK_FILE_NAME);
        fs::write(&lock_path, r#"{"skills":{}}"#)?;

        //* When
        SkillsLock::default().save(skills_dir.path())?;

        //* Then
        assert!(
            !lock_path.exists(),
            "an empty lock should not leave a lock file behind"
        );

        Ok(())
    }

    #[test]
    fn load_with_path_traversal_skill_name_fails() -> Result<()> {
        //* Given
        let skills_dir = TempDir::new()?;
        let tampered_lock =
            r#"{"skills":{"../..":{"repo":"edgeandnode/amp","ref":"v0.1.0","commit":"0123"}}}"#;
        fs::write(skills_dir.path().join(LOCK_FILE_NAME), tampered_lock)?;

        //* When
        let result = SkillsLock::load(skills_dir.path());

        //* Then
        assert!(
            matches!(result, Err(LoadSkillsLockError::Parse { .. })),
            "a lock file naming a path outside the skills directory must be rejected"
        );

        Ok(())
    }
}
