//! Agent skills distributed in the amp repository.
//!
//! Skills live in the repository's `skills/` directory and are not published as
//! release assets, so they are read from the repository source tarball of a git
//! ref. A skill's `SKILL.md` frontmatter can link files that live elsewhere in
//! the repository, such as feature docs and JSON schemas. Those files are
//! copied into the skill's `references/` and `assets/` directories on install,
//! so the installed skill is self-contained.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    path::{Path, PathBuf},
};

use fs_err as fs;

mod agent;
mod archive;
mod frontmatter;
mod lock;
mod skill_name;

// Only tests inspect the lock file by name
#[cfg(test)]
pub(crate) use self::lock::LOCK_FILE_NAME;
pub use self::{
    agent::{Agent, Scope},
    skill_name::{InvalidSkillNameError, SkillName},
};
pub(crate) use self::{
    archive::{ArchiveFile, ReadFilesError},
    frontmatter::{Frontmatter, ParseFrontmatterError},
    lock::{InstalledSkill, LoadSkillsLockError, SaveSkillsLockError, SkillsLock},
};

/// Repository directory that holds one subdirectory per skill.
const SKILLS_REPO_DIR: &str = "skills/";

/// The file that makes a directory a skill.
const SKILL_FILE_NAME: &str = "SKILL.md";

/// Skill directory that receives files linked under `metadata.references`.
const REFERENCES_DIR: &str = "references";

/// Skill directory that receives files linked under `metadata.assets`.
const ASSETS_DIR: &str = "assets";

/// A skill ready to install, with its linked files already copied in.
#[derive(Debug, Clone)]
pub(crate) struct Skill {
    /// Skill name, which is also its directory name.
    pub name: SkillName,
    /// What the skill does and when to use it.
    pub description: String,
    /// Files keyed by their path inside the skill directory (e.g.
    /// `SKILL.md`, `references/sql.md`).
    pub files: BTreeMap<String, ArchiveFile>,
}

/// Read every skill from a repository source tarball file.
///
/// A skill is a `skills/<name>/` directory containing a `SKILL.md`. Files
/// linked from the frontmatter `metadata` are copied into the skill as
/// `references/<file name>` and `assets/<file name>`.
///
/// # Errors
///
/// Fails when the tarball cannot be read, a skill directory name is not a
/// valid [`SkillName`], a `SKILL.md` is not UTF-8 or has invalid frontmatter,
/// or the frontmatter links a path that is not in the repository. See
/// [`LoadSkillsError`].
pub(crate) fn load_skills(tarball: &File) -> Result<Vec<Skill>, LoadSkillsError> {
    let skills_dir_files =
        archive::read_files(tarball, archive::Selection::UnderDir(SKILLS_REPO_DIR))
            .map_err(LoadSkillsError::ReadSkillsDir)?;

    let mut files_by_skill: BTreeMap<String, BTreeMap<String, ArchiveFile>> = BTreeMap::new();
    for (repo_path, file) in skills_dir_files {
        let Some(path_in_skills_dir) = repo_path.strip_prefix(SKILLS_REPO_DIR) else {
            continue;
        };

        // Files directly inside `skills/`, such as README.md, belong to no skill
        let Some((skill_name, path_in_skill)) = path_in_skills_dir.split_once('/') else {
            continue;
        };

        files_by_skill
            .entry(skill_name.to_string())
            .or_default()
            .insert(path_in_skill.to_string(), file);
    }

    // Parse every SKILL.md first, so all linked files are read in one pass
    let mut skills = Vec::new();
    let mut linked_paths = BTreeSet::new();
    for (dir_name, files) in files_by_skill {
        let Some(skill_md) = files.get(SKILL_FILE_NAME) else {
            continue;
        };

        let name: SkillName = match dir_name.parse() {
            Ok(name) => name,
            Err(err) => return Err(LoadSkillsError::InvalidSkillDir(err)),
        };
        let skill_md = match std::str::from_utf8(&skill_md.contents) {
            Ok(skill_md) => skill_md,
            Err(err) => {
                return Err(LoadSkillsError::SkillMdNotUtf8 {
                    skill: name,
                    source: err,
                });
            }
        };
        let frontmatter: Frontmatter = match skill_md.parse() {
            Ok(frontmatter) => frontmatter,
            Err(err) => {
                return Err(LoadSkillsError::InvalidFrontmatter {
                    skill: name,
                    source: err,
                });
            }
        };

        linked_paths.extend(frontmatter.references.iter().cloned());
        linked_paths.extend(frontmatter.assets.iter().cloned());

        skills.push((name, frontmatter, files));
    }

    let linked_files = archive::read_files(tarball, archive::Selection::Paths(&linked_paths))
        .map_err(LoadSkillsError::ReadLinkedFiles)?;

    let mut loaded_skills = Vec::new();
    for (name, frontmatter, mut files) in skills {
        copy_linked_files(
            &mut files,
            &name,
            REFERENCES_DIR,
            &frontmatter.references,
            &linked_files,
        )
        .map_err(LoadSkillsError::LinkedFileNotFound)?;
        copy_linked_files(
            &mut files,
            &name,
            ASSETS_DIR,
            &frontmatter.assets,
            &linked_files,
        )
        .map_err(LoadSkillsError::LinkedFileNotFound)?;

        loaded_skills.push(Skill {
            name,
            description: frontmatter.description,
            files,
        });
    }

    Ok(loaded_skills)
}

/// Errors from reading skills out of a repository source tarball
#[derive(Debug, thiserror::Error)]
pub(crate) enum LoadSkillsError {
    /// The files under `skills/` could not be read from the tarball
    #[error("Failed to read the skills directory from the source tarball")]
    ReadSkillsDir(#[source] ReadFilesError),

    /// A skill directory's name is not a valid skill name
    ///
    /// Occurs when the repository has a skill directory whose name contains a
    /// backslash, which could not be installed as a single directory.
    #[error("Invalid skill directory in the repository")]
    InvalidSkillDir(#[source] InvalidSkillNameError),

    /// A skill's `SKILL.md` is not valid UTF-8
    #[error("skills/{skill}/SKILL.md is not valid UTF-8")]
    SkillMdNotUtf8 {
        skill: SkillName,
        #[source]
        source: std::str::Utf8Error,
    },

    /// A skill's `SKILL.md` frontmatter could not be parsed
    #[error("Invalid frontmatter in skills/{skill}/SKILL.md")]
    InvalidFrontmatter {
        skill: SkillName,
        #[source]
        source: ParseFrontmatterError,
    },

    /// The files linked from skill frontmatter could not be read from the tarball
    #[error("Failed to read linked files from the source tarball")]
    ReadLinkedFiles(#[source] ReadFilesError),

    /// A skill links a file that does not exist
    ///
    /// No skill is returned, so nothing is installed with broken links.
    #[error("A skill links a file that does not exist")]
    LinkedFileNotFound(#[source] LinkedFileNotFoundError),
}

/// A skills directory and the agents that read from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillsDir {
    /// Absolute path of the directory skills are installed into.
    pub path: PathBuf,
    /// The agents that read skills from this directory.
    pub agents: Vec<Agent>,
}

/// Group agents by the skills directory they read from.
///
/// Agents that share a directory (such as Codex and Cursor in a project) get a
/// single entry, so each directory is installed into only once.
pub(crate) fn skills_dirs(agents: &[Agent], scope: Scope, base_dir: &Path) -> Vec<SkillsDir> {
    let mut dirs: Vec<SkillsDir> = Vec::new();

    for &agent in agents {
        let path = agent::skills_dir(agent, scope, base_dir);

        match dirs.iter_mut().find(|dir| dir.path == path) {
            Some(dir) => {
                if !dir.agents.contains(&agent) {
                    dir.agents.push(agent);
                }
            }
            None => dirs.push(SkillsDir {
                path,
                agents: vec![agent],
            }),
        }
    }

    dirs
}

/// Install a skill into `<skills_dir>/<skill name>`, replacing any previous
/// copy.
///
/// # Errors
///
/// Fails when the skills directory, the staged skill files, or their
/// permissions cannot be written, when the previous copy cannot be removed, or
/// when the staged skill cannot be moved into place. See [`InstallSkillError`].
pub(crate) fn install_skill(skills_dir: &Path, skill: &Skill) -> Result<(), InstallSkillError> {
    fs::create_dir_all(skills_dir).map_err(InstallSkillError::CreateSkillsDir)?;

    // Write the skill to a hidden staging directory next to its final location,
    // so agents never load a half-written skill
    let staging_dir = tempfile::Builder::new()
        .prefix(".amp-skill-")
        .tempdir_in(skills_dir)
        .map_err(InstallSkillError::CreateStagingDir)?;

    for (path_in_skill, file) in &skill.files {
        let file_path = staging_dir.path().join(path_in_skill);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(InstallSkillError::CreateFileDir)?;
        }
        fs::write(&file_path, &file.contents).map_err(InstallSkillError::WriteFile)?;

        #[cfg(unix)]
        if file.is_executable {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&file_path, std::fs::Permissions::from_mode(0o755))
                .map_err(InstallSkillError::SetExecutable)?;
        }
    }

    uninstall_skill(skills_dir, &skill.name).map_err(InstallSkillError::RemovePreviousCopy)?;

    let skill_dir = skills_dir.join(skill.name.as_str());

    // `keep()` stops the TempDir from deleting the directory on drop, so it has
    // to be cleaned up by hand if the rename fails
    let staged_path = staging_dir.keep();
    if let Err(err) = fs::rename(&staged_path, &skill_dir) {
        // The rename error is the one the user needs; if this cleanup also
        // fails, a hidden `.amp-skill-*` directory is left behind, which agents
        // ignore and the next install does not trip over
        let _ = fs::remove_dir_all(&staged_path);
        return Err(InstallSkillError::MoveIntoPlace {
            skill_dir,
            source: err,
        });
    }

    Ok(())
}

/// Errors from installing a skill into a skills directory
///
/// The previous copy of the skill is removed just before the new one is moved
/// in, so a failure in [`Self::MoveIntoPlace`] leaves the skill uninstalled;
/// running the install again is safe.
#[derive(Debug, thiserror::Error)]
pub(crate) enum InstallSkillError {
    /// The skills directory could not be created
    #[error("Failed to create skills directory")]
    CreateSkillsDir(#[source] std::io::Error),

    /// The hidden staging directory could not be created
    #[error("Failed to create skill staging directory")]
    CreateStagingDir(#[source] std::io::Error),

    /// A directory inside the staged skill could not be created
    #[error("Failed to create skill directory")]
    CreateFileDir(#[source] std::io::Error),

    /// A skill file could not be written to the staging directory
    #[error("Failed to write skill file")]
    WriteFile(#[source] std::io::Error),

    /// A skill script could not be made executable
    #[error("Failed to make skill file executable")]
    SetExecutable(#[source] std::io::Error),

    /// The previously installed copy could not be removed
    #[error("Failed to remove the previous copy of the skill")]
    RemovePreviousCopy(#[source] UninstallSkillError),

    /// The staged skill could not be moved into the skills directory
    #[error("Failed to move skill into {}", skill_dir.display())]
    MoveIntoPlace {
        skill_dir: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Delete `<skills_dir>/<skill name>` if it exists.
///
/// # Errors
///
/// Fails when the skill's metadata cannot be read for a reason other than it
/// not existing, or when the skill cannot be deleted. See
/// [`UninstallSkillError`].
pub(crate) fn uninstall_skill(
    skills_dir: &Path,
    skill_name: &SkillName,
) -> Result<(), UninstallSkillError> {
    let skill_dir = skills_dir.join(skill_name.as_str());

    // `symlink_metadata` does not follow symlinks, so a symlinked skill is
    // unlinked instead of deleting the directory it points to
    let metadata = match fs::symlink_metadata(&skill_dir) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(UninstallSkillError::ReadMetadata(err)),
    };

    if metadata.is_dir() {
        fs::remove_dir_all(&skill_dir).map_err(UninstallSkillError::RemoveDir)?;
    } else {
        fs::remove_file(&skill_dir).map_err(UninstallSkillError::RemoveFile)?;
    }

    Ok(())
}

/// Errors from deleting an installed skill
#[derive(Debug, thiserror::Error)]
pub(crate) enum UninstallSkillError {
    /// The skill's metadata could not be read
    ///
    /// Possible causes:
    /// - Permission denied on the skills directory
    #[error("Failed to read skill metadata")]
    ReadMetadata(#[source] std::io::Error),

    /// The skill directory could not be deleted
    ///
    /// Some of its files may already be gone.
    #[error("Failed to remove skill directory")]
    RemoveDir(#[source] std::io::Error),

    /// The skill was a file or symlink and could not be deleted
    #[error("Failed to remove skill")]
    RemoveFile(#[source] std::io::Error),
}

/// Copy linked repository files into `<link_dir>/<file name>` inside a skill.
///
/// A link keeps only the file name, so `docs/feat/sql.md` listed under
/// `metadata.references` becomes `references/sql.md`.
fn copy_linked_files(
    skill_files: &mut BTreeMap<String, ArchiveFile>,
    skill_name: &SkillName,
    link_dir: &str,
    repo_paths: &[String],
    linked_files: &BTreeMap<String, ArchiveFile>,
) -> Result<(), LinkedFileNotFoundError> {
    for repo_path in repo_paths {
        let Some(file) = linked_files.get(repo_path) else {
            return Err(LinkedFileNotFoundError {
                skill: skill_name.clone(),
                path: repo_path.clone(),
            });
        };

        let file_name = match repo_path.rsplit_once('/') {
            Some((_, file_name)) => file_name,
            None => repo_path,
        };

        skill_files.insert(format!("{}/{}", link_dir, file_name), file.clone());
    }

    Ok(())
}

/// A skill's frontmatter links a repository path that does not exist
///
/// Occurs when `metadata.references` or `metadata.assets` in a `SKILL.md` lists
/// a path that was moved or deleted in the repository.
#[derive(Debug, thiserror::Error)]
#[error("Skill {skill} links {path}, which does not exist in the repository")]
pub(crate) struct LinkedFileNotFoundError {
    pub skill: SkillName,
    pub path: String,
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::TempDir;

    use super::*;

    /// Write a GitHub-style source tarball, with every file under a top-level
    /// `owner-repo-commit` directory, to a temporary file.
    fn source_tarball(files: &[(&str, &str)]) -> Result<File> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);

        for (repo_path, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(u64::try_from(contents.len())?);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(
                &mut header,
                format!("edgeandnode-amp-0123abc/{}", repo_path),
                contents.as_bytes(),
            )?;
        }

        let encoder = builder.into_inner()?;
        let tarball = encoder.finish()?;

        let mut file = tempfile::tempfile()?;
        std::io::Write::write_all(&mut file, &tarball)?;
        Ok(file)
    }

    fn file_text(skill: &Skill, path_in_skill: &str) -> Option<String> {
        skill
            .files
            .get(path_in_skill)
            .map(|file| String::from_utf8_lossy(&file.contents).into_owned())
    }

    #[test]
    fn skills_dirs_with_all_agents_in_project_scope_groups_shared_dirs() {
        //* Given
        let agents = [Agent::ClaudeCode, Agent::Codex, Agent::Cursor];
        let base_dir = Path::new("/work/project");

        //* When
        let dirs = skills_dirs(&agents, Scope::Project, base_dir);

        //* Then
        assert_eq!(
            dirs,
            vec![
                SkillsDir {
                    path: "/work/project/.claude/skills".into(),
                    agents: vec![Agent::ClaudeCode],
                },
                SkillsDir {
                    path: "/work/project/.agents/skills".into(),
                    agents: vec![Agent::Codex, Agent::Cursor],
                },
            ],
            "codex and cursor should share the .agents/skills directory"
        );
    }

    #[test]
    fn load_skills_with_linked_files_copies_them_into_skill() -> Result<()> {
        //* Given
        let tarball = source_tarball(&[
            ("skills/README.md", "# Skills\n"),
            (
                "skills/amp-providers/SKILL.md",
                "---\nname: amp-providers\ndescription: Providers.\nmetadata:\n  references: docs/feat/provider.md\n  assets: docs/schemas/evm-rpc.spec.json\n---\n",
            ),
            ("skills/amp-providers/references/workflow-setup.md", "setup"),
            ("docs/feat/provider.md", "provider doc"),
            ("docs/schemas/evm-rpc.spec.json", "{}"),
            ("docs/feat/unrelated.md", "unrelated"),
        ])?;

        //* When
        let skills = load_skills(&tarball)?;

        //* Then
        assert_eq!(skills.len(), 1, "only directories with SKILL.md are skills");
        let skill = &skills[0];
        assert_eq!(
            skill.name.as_str(),
            "amp-providers",
            "the skill name should come from its directory"
        );
        assert_eq!(
            skill.files.keys().collect::<Vec<_>>(),
            vec![
                "SKILL.md",
                "assets/evm-rpc.spec.json",
                "references/provider.md",
                "references/workflow-setup.md",
            ],
            "linked files should sit next to the skill's own files"
        );
        assert_eq!(
            file_text(skill, "references/provider.md").as_deref(),
            Some("provider doc"),
            "linked reference should hold the repository file contents"
        );

        Ok(())
    }

    #[test]
    fn install_skill_with_previous_copy_replaces_its_files() -> Result<()> {
        //* Given
        let skills_dir = TempDir::new()?;
        let stale_file = skills_dir.path().join("amp-query/references/stale.md");
        fs::create_dir_all(skills_dir.path().join("amp-query/references"))?;
        fs::write(&stale_file, "stale")?;

        let tarball = source_tarball(&[(
            "skills/amp-query/SKILL.md",
            "---\nname: amp-query\ndescription: Query.\n---\n",
        )])?;
        let skills = load_skills(&tarball)?;

        //* When
        install_skill(skills_dir.path(), &skills[0])?;

        //* Then
        assert!(
            skills_dir.path().join("amp-query/SKILL.md").exists(),
            "new skill files should be installed"
        );
        assert!(
            !stale_file.exists(),
            "files from the previous copy should be gone"
        );

        Ok(())
    }

    #[test]
    fn load_skills_with_missing_linked_file_fails() -> Result<()> {
        //* Given
        let tarball = source_tarball(&[(
            "skills/amp-query/SKILL.md",
            "---\nname: amp-query\ndescription: Query.\nmetadata:\n  references: docs/feat/missing.md\n---\n",
        )])?;

        //* When
        let result = load_skills(&tarball);

        //* Then
        assert!(
            matches!(
                &result,
                Err(LoadSkillsError::LinkedFileNotFound(LinkedFileNotFoundError { path, .. }))
                    if path == "docs/feat/missing.md"
            ),
            "error should name the missing file, got: {result:?}"
        );

        Ok(())
    }
}
