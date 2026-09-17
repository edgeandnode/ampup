//! Integration tests for the `ampup skills` commands.
//!
//! Each test drives a command end to end — resolve the git ref, download the
//! repository source tarball, read the skills, copy linked files, and write
//! the skills directories and lock file — against an in-process mock GitHub
//! server, so no network is required. Tests call the command functions that
//! take an injected GitHub client and install location, which are exactly what
//! the CLI entry points call after building those from flags.

use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use anyhow::Result;
use flate2::{Compression, write::GzEncoder};
use fs_err as fs;
use tar::{Builder, Header};
use tempfile::TempDir;
use tokio_util::task::AbortOnDropHandle;

use super::{fixtures::TempInstallDir, mock_github};
use crate::{
    commands::skills::{
        self, AddSkillsError, DownloadSnapshotError, InstallFromSnapshotError,
        LoadSnapshotSkillsError, RemoveSkillsError, ResolveSourceRefError, SelectSkillsError,
        SkillAction, SkillSelection, SkillsLocation, SourceRef,
    },
    config::Config,
    github::{GitHubClient, GitHubError},
    skills::{
        Agent, InstalledSkill, LOCK_FILE_NAME, LinkedFileNotFoundError, LoadSkillsError, Scope,
        SkillName, SkillsLock,
    },
};

/// Commit the mock GitHub API resolves `v1.0.0` to.
const COMMIT_V1: &str = "1111111111111111111111111111111111111111";

/// Commit the mock GitHub API resolves `v2.0.0` to.
const COMMIT_V2: &str = "2222222222222222222222222222222222222222";

/// Head commit of the mock pull request.
const COMMIT_PR: &str = "4242424242424242424242424242424242424242";

/// Number of the mock pull request.
const PR_NUMBER: u32 = 42;

/// Repository name reported by a mock GitHub client.
const MOCK_REPO: &str = "test/repo";

mod list_available_skills {
    use super::*;

    #[tokio::test]
    async fn list_available_skills_with_ref_returns_names_and_descriptions() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;

        //* When
        let available = skills::list_available_skills(
            &github,
            &env.config()?,
            SourceRef::Git("v1.0.0".to_string()),
        )
        .await?;

        //* Then
        let skills: Vec<(&str, &str)> = available
            .skills
            .iter()
            .map(|skill| (skill.name.as_str(), skill.description.as_str()))
            .collect();
        assert_eq!(
            skills,
            vec![
                ("amp-query", "Query Amp data."),
                ("amp-setup", "Set up Amp.")
            ],
            "every skill should be listed with its frontmatter description"
        );
        assert!(
            !env.project_path(".claude").exists() && !env.project_path(".agents").exists(),
            "listing should not install anything"
        );

        Ok(())
    }
}

mod add_skills {
    use super::*;

    #[tokio::test]
    async fn add_skills_without_ref_installs_active_version() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        env.set_active_version("v1.0.0")?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;

        //* When
        let changes = skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::ActiveVersion,
            named(&["amp-query"])?,
            vec![Agent::ClaudeCode],
        )
        .await?;

        //* Then
        let skills_dir = env.project_path(".claude/skills");
        assert_eq!(
            files_under(&skills_dir)?,
            vec![
                LOCK_FILE_NAME,
                "amp-query/SKILL.md",
                "amp-query/assets/evm-rpc.spec.json",
                "amp-query/references/app-ampsql.md",
                "amp-query/references/evm-rpc.md",
                "amp-query/references/workflow-decoding.md",
                "amp-query/scripts/check.sh",
            ],
            "only the selected skill should be installed, with its linked files in references/ and assets/"
        );
        assert_eq!(
            fs::read_to_string(skills_dir.join("amp-query/references/app-ampsql.md"))?,
            "ampsql doc v1.0.0",
            "linked files should come from the active version"
        );
        assert_eq!(
            SkillsLock::load(&skills_dir)?
                .skills
                .get(&"amp-query".parse()?),
            Some(&installed_from("v1.0.0", COMMIT_V1)),
            "the lock should record the ref and the commit it resolved to"
        );
        assert_eq!(
            changes
                .changes
                .iter()
                .map(|change| (
                    change.action,
                    change.skill.as_str(),
                    change.skills_dir.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![(SkillAction::Installed, "amp-query", ".claude/skills")],
            "the result should report the installed skill and where it went"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_with_all_for_codex_and_cursor_uses_shared_dir() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;

        //* When
        skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::Git("v1.0.0".to_string()),
            SkillSelection::All,
            vec![Agent::Codex, Agent::Cursor],
        )
        .await?;

        //* Then
        let skills_dir = env.project_path(".agents/skills");
        assert_eq!(
            installed_names(&skills_dir)?,
            vec!["amp-query", "amp-setup"],
            "every skill should be installed, and README.md is not a skill"
        );
        assert!(
            skills_dir.join("amp-setup/SKILL.md").exists()
                && skills_dir.join("amp-query/SKILL.md").exists(),
            "codex and cursor should share the project .agents/skills directory"
        );
        assert!(
            !env.project_path(".claude").exists(),
            "agents that were not selected should get nothing"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_with_global_scope_installs_in_home_dir() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let home_dir = TempDir::new()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;
        let global = SkillsLocation {
            scope: Scope::Global,
            base_dir: home_dir.path().to_path_buf(),
        };

        //* When
        skills::add_skills(
            &github,
            &env.config()?,
            &global,
            SourceRef::Git("v1.0.0".to_string()),
            named(&["amp-setup"])?,
            vec![Agent::Cursor],
        )
        .await?;

        //* Then
        assert!(
            home_dir
                .path()
                .join(".cursor/skills/amp-setup/SKILL.md")
                .exists(),
            "global cursor skills should be installed in ~/.cursor/skills"
        );
        assert!(
            !home_dir.path().join(".agents").exists(),
            "the project-only .agents directory should not be used for global skills"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_with_explicit_ref_installs_that_ref() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        env.set_active_version("v1.0.0")?;
        let (github, _server) = start_mock_github(vec![MockRef::v1(), MockRef::v2()]).await?;

        //* When
        skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::Git("v2.0.0".to_string()),
            named(&["amp-setup"])?,
            vec![Agent::ClaudeCode],
        )
        .await?;

        //* Then
        let skills_dir = env.project_path(".claude/skills");
        assert_eq!(
            fs::read_to_string(skills_dir.join("amp-setup/SKILL.md"))?,
            "---\nname: amp-setup\ndescription: Set up Amp.\n---\n# Setup v2.0.0\n",
            "--ref should win over the active ampd version"
        );
        assert_eq!(
            SkillsLock::load(&skills_dir)?
                .skills
                .get(&"amp-setup".parse()?),
            Some(&installed_from("v2.0.0", COMMIT_V2)),
            "the lock should record the explicit ref"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_with_pull_request_installs_its_head_commit() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        env.set_active_version("v1.0.0")?;
        let (github, _server) =
            start_mock_github(vec![MockRef::v1(), MockRef::pull_request()]).await?;
        let pull_request = SourceRef::PullRequest(NonZeroU32::try_from(PR_NUMBER)?);

        //* When
        skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            pull_request,
            named(&["amp-query"])?,
            vec![Agent::ClaudeCode],
        )
        .await?;

        //* Then
        let skills_dir = env.project_path(".claude/skills");
        assert_eq!(
            fs::read_to_string(skills_dir.join("amp-query/references/app-ampsql.md"))?,
            "ampsql doc pull request",
            "skills should come from the pull request head, not the active version"
        );
        assert_eq!(
            SkillsLock::load(&skills_dir)?
                .skills
                .get(&"amp-query".parse()?),
            Some(&installed_from("pr/42", COMMIT_PR)),
            "the lock should record the pull request and its head commit"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_with_existing_user_skill_leaves_it_untouched() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let user_skill_md = env.project_path(".claude/skills/amp-setup/SKILL.md");
        fs::create_dir_all(env.project_path(".claude/skills/amp-setup"))?;
        fs::write(&user_skill_md, "my own amp-setup skill")?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;

        //* When
        skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::Git("v1.0.0".to_string()),
            SkillSelection::All,
            vec![Agent::ClaudeCode],
        )
        .await?;

        //* Then
        assert_eq!(
            fs::read_to_string(&user_skill_md)?,
            "my own amp-setup skill",
            "a skill ampup did not install must never be overwritten"
        );
        assert_eq!(
            installed_names(&env.project_path(".claude/skills"))?,
            vec!["amp-query"],
            "the other skills should still be installed, and the skipped one not recorded"
        );

        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn add_skills_with_executable_script_keeps_it_executable() -> Result<()> {
        use std::os::unix::fs::PermissionsExt as _;

        //* Given
        const EXECUTE_BITS: u32 = 0o111;
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;

        //* When
        skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::Git("v1.0.0".to_string()),
            named(&["amp-query"])?,
            vec![Agent::ClaudeCode],
        )
        .await?;

        //* Then
        let skills_dir = env.project_path(".claude/skills");
        let script_mode = fs::metadata(skills_dir.join("amp-query/scripts/check.sh"))?
            .permissions()
            .mode();
        let doc_mode = fs::metadata(skills_dir.join("amp-query/SKILL.md"))?
            .permissions()
            .mode();
        assert_ne!(
            script_mode & EXECUTE_BITS,
            0,
            "skill scripts should stay runnable"
        );
        assert_eq!(
            doc_mode & EXECUTE_BITS,
            0,
            "skill docs should not become executable"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_without_ref_or_active_version_fails() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        // No mock server: the command must fail before making any request
        let github = GitHubClient::with_api_base("http://127.0.0.1:1".to_string(), None)?;

        //* When
        let result = skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::ActiveVersion,
            named(&["amp-setup"])?,
            vec![Agent::ClaudeCode],
        )
        .await;

        //* Then
        assert!(
            matches!(
                result,
                Err(AddSkillsError::ResolveSourceRef(
                    ResolveSourceRefError::NoActiveVersion
                ))
            ),
            "error should explain that no ampd version is active, got: {result:?}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_with_unknown_ref_fails_without_writing() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;

        //* When
        let result = skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::Git("v9.9.9".to_string()),
            named(&["amp-setup"])?,
            vec![Agent::ClaudeCode],
        )
        .await;

        //* Then
        let is_ref_not_found = match &result {
            Err(AddSkillsError::Download(DownloadSnapshotError::ResolveCommit(err))) => matches!(
                err.downcast_ref::<GitHubError>(),
                Some(GitHubError::RefNotFound { git_ref, .. }) if git_ref == "v9.9.9"
            ),
            _ => false,
        };
        assert!(
            is_ref_not_found,
            "error should name the missing ref, got: {result:?}"
        );
        assert!(
            !env.project_path(".claude").exists(),
            "nothing should be written when the ref does not exist"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_with_unknown_pull_request_fails_without_writing() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::pull_request()]).await?;
        let unknown_pull_request = SourceRef::PullRequest(NonZeroU32::try_from(999)?);

        //* When
        let result = skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            unknown_pull_request,
            named(&["amp-setup"])?,
            vec![Agent::ClaudeCode],
        )
        .await;

        //* Then
        let is_pull_request_not_found = match &result {
            Err(AddSkillsError::Download(DownloadSnapshotError::ResolveCommit(err))) => matches!(
                err.downcast_ref::<GitHubError>(),
                Some(GitHubError::PullRequestNotFound { number, .. }) if number.get() == 999
            ),
            _ => false,
        };
        assert!(
            is_pull_request_not_found,
            "error should name the missing pull request, got: {result:?}"
        );
        assert!(
            !env.project_path(".claude").exists(),
            "nothing should be written when the pull request does not exist"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_with_unknown_skill_name_fails_without_installing() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;

        //* When
        let result = skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::Git("v1.0.0".to_string()),
            named(&["amp-setup", "amp-typo"])?,
            vec![Agent::ClaudeCode],
        )
        .await;

        //* Then
        assert!(
            matches!(
                &result,
                Err(AddSkillsError::Install(InstallFromSnapshotError::SelectSkills(
                    SelectSkillsError::SkillNotFound { name, available, .. }
                ))) if name.as_str() == "amp-typo" && available.len() == 2
            ),
            "error should name the missing skill and list the available ones, got: {result:?}"
        );
        assert!(
            !env.project_path(".claude").exists(),
            "valid skills in the same request should not be installed either"
        );

        Ok(())
    }

    #[tokio::test]
    async fn add_skills_with_broken_linked_file_fails_without_installing() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let mut files = amp_repo("v1.0.0");
        files.retain(|file| file.path != "docs/feat/app-ampsql.md");
        let (github, _server) = start_mock_github(vec![MockRef {
            git_ref: "v1.0.0",
            commit: COMMIT_V1,
            files,
            pull_request: None,
        }])
        .await?;

        //* When
        let result = skills::add_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::Git("v1.0.0".to_string()),
            SkillSelection::All,
            vec![Agent::ClaudeCode],
        )
        .await;

        //* Then
        assert!(
            matches!(
                &result,
                Err(AddSkillsError::Install(InstallFromSnapshotError::LoadSkills(
                    LoadSnapshotSkillsError::LoadSkills(LoadSkillsError::LinkedFileNotFound(
                        LinkedFileNotFoundError { skill, path }
                    ))
                ))) if skill.as_str() == "amp-query" && path == "docs/feat/app-ampsql.md"
            ),
            "error should name the skill and the missing file, got: {result:?}"
        );
        assert!(
            !env.project_path(".claude").exists(),
            "no skill should be installed with broken links"
        );

        Ok(())
    }
}

mod list_installed_skills {
    use super::*;

    #[tokio::test]
    async fn list_installed_skills_with_installed_skills_groups_them_by_dir() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;
        install_v1_for_claude_code(&env, &github, &["amp-query", "amp-setup"]).await?;

        //* When
        let installed = skills::list_installed_skills(&env.project(), &[])?;

        //* Then
        let dirs: Vec<(&str, Vec<&str>)> = installed
            .dirs
            .iter()
            .map(|dir| {
                let names = dir.skills.iter().map(|skill| skill.name.as_str()).collect();
                (dir.skills_dir.as_str(), names)
            })
            .collect();
        assert_eq!(
            dirs,
            vec![(".claude/skills", vec!["amp-query", "amp-setup"])],
            "only directories with installed skills should be listed"
        );

        Ok(())
    }
}

mod update_skills {
    use super::*;

    #[tokio::test]
    async fn update_skills_without_ref_reinstalls_active_version() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let mut files_v2 = amp_repo("v2.0.0");
        files_v2.retain(|file| !file.path.ends_with("workflow-decoding.md"));
        let (github, _server) = start_mock_github(vec![
            MockRef::v1(),
            MockRef {
                git_ref: "v2.0.0",
                commit: COMMIT_V2,
                files: files_v2,
                pull_request: None,
            },
        ])
        .await?;
        install_v1_for_claude_code(&env, &github, &["amp-query"]).await?;
        env.set_active_version("v2.0.0")?;

        //* When
        skills::update_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::ActiveVersion,
            Vec::new(),
            Vec::new(),
        )
        .await?;

        //* Then
        let skills_dir = env.project_path(".claude/skills");
        assert_eq!(
            fs::read_to_string(skills_dir.join("amp-query/references/app-ampsql.md"))?,
            "ampsql doc v2.0.0",
            "linked files should be refreshed from the new active version"
        );
        assert!(
            !skills_dir
                .join("amp-query/references/workflow-decoding.md")
                .exists(),
            "files dropped from the skill in the new version should be removed"
        );
        assert_eq!(
            SkillsLock::load(&skills_dir)?
                .skills
                .get(&"amp-query".parse()?),
            Some(&installed_from("v2.0.0", COMMIT_V2)),
            "the lock should record the new ref and commit"
        );

        Ok(())
    }

    #[tokio::test]
    async fn update_skills_with_skill_filter_updates_only_that_skill() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1(), MockRef::v2()]).await?;
        install_v1_for_claude_code(&env, &github, &["amp-query", "amp-setup"]).await?;

        //* When
        skills::update_skills(
            &github,
            &env.config()?,
            &env.project(),
            SourceRef::Git("v2.0.0".to_string()),
            skill_names(&["amp-query"])?,
            Vec::new(),
        )
        .await?;

        //* Then
        let lock = SkillsLock::load(&env.project_path(".claude/skills"))?;
        assert_eq!(
            lock.skills.get(&"amp-query".parse()?),
            Some(&installed_from("v2.0.0", COMMIT_V2)),
            "the requested skill should be updated"
        );
        assert_eq!(
            lock.skills.get(&"amp-setup".parse()?),
            Some(&installed_from("v1.0.0", COMMIT_V1)),
            "other installed skills should stay at their version"
        );

        Ok(())
    }
}

mod remove_skills {
    use super::*;

    #[tokio::test]
    async fn remove_skills_with_skill_name_removes_only_that_skill() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;
        install_v1_for_claude_code(&env, &github, &["amp-query", "amp-setup"]).await?;

        //* When
        skills::remove_skills(&env.project(), named(&["amp-query"])?, &[], true)?;

        //* Then
        let skills_dir = env.project_path(".claude/skills");
        assert!(
            !skills_dir.join("amp-query").exists(),
            "the removed skill's directory should be deleted"
        );
        assert!(
            skills_dir.join("amp-setup/SKILL.md").exists(),
            "other skills should stay installed"
        );
        assert_eq!(
            installed_names(&skills_dir)?,
            vec!["amp-setup"],
            "the removed skill should be dropped from the lock"
        );

        Ok(())
    }

    #[tokio::test]
    async fn remove_skills_with_all_keeps_user_skills() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let (github, _server) = start_mock_github(vec![MockRef::v1()]).await?;
        install_v1_for_claude_code(&env, &github, &["amp-query", "amp-setup"]).await?;
        let user_skill_md = env.project_path(".claude/skills/my-skill/SKILL.md");
        fs::create_dir_all(env.project_path(".claude/skills/my-skill"))?;
        fs::write(&user_skill_md, "my own skill")?;

        //* When
        skills::remove_skills(&env.project(), SkillSelection::All, &[], true)?;

        //* Then
        assert_eq!(
            files_under(&env.project_path(".claude/skills"))?,
            vec!["my-skill/SKILL.md"],
            "only ampup skills and the lock file should be removed"
        );

        Ok(())
    }

    #[test]
    fn remove_skills_with_tampered_lock_fails_without_deleting() -> Result<()> {
        //* Given
        let env = TestEnv::create()?;
        let project_file = env.project_path("Cargo.toml");
        fs::write(&project_file, "[workspace]")?;
        // A lock file committed to the project that names the project root
        fs::create_dir_all(env.project_path(".claude/skills"))?;
        fs::write(
            env.project_path(".claude/skills").join(LOCK_FILE_NAME),
            r#"{"skills":{"../..":{"repo":"test/repo","ref":"v1.0.0","commit":"1111"}}}"#,
        )?;

        //* When
        let result = skills::remove_skills(&env.project(), SkillSelection::All, &[], true);

        //* Then
        assert!(
            matches!(result, Err(RemoveSkillsError::LoadLock(_))),
            "a lock file naming a path outside the skills directory must be rejected, got: {result:?}"
        );
        assert!(
            project_file.exists(),
            "nothing outside the skills directory should be deleted"
        );

        Ok(())
    }
}

/// A file in the mock amp repository.
struct RepoFile {
    path: String,
    contents: String,
    is_executable: bool,
}

impl RepoFile {
    fn new(path: &str, contents: impl Into<String>) -> Self {
        Self {
            path: path.to_string(),
            contents: contents.into(),
            is_executable: false,
        }
    }

    fn executable(path: &str, contents: impl Into<String>) -> Self {
        Self {
            path: path.to_string(),
            contents: contents.into(),
            is_executable: true,
        }
    }
}

/// The amp repository at `version`, shaped like the real one: two skills, a
/// README that is not a skill, feature docs and schemas that `amp-query` links
/// from its frontmatter, and a doc nothing links.
///
/// `version` is written into the file contents so tests can tell which ref a
/// file was installed from.
fn amp_repo(version: &str) -> Vec<RepoFile> {
    vec![
        RepoFile::new("skills/README.md", "# Amp User Skills\n"),
        RepoFile::new(
            "skills/amp-setup/SKILL.md",
            format!("---\nname: amp-setup\ndescription: Set up Amp.\n---\n# Setup {version}\n"),
        ),
        RepoFile::new(
            "skills/amp-query/SKILL.md",
            format!(
                "---\n\
                 name: amp-query\n\
                 description: Query Amp data.\n\
                 metadata:\n  \
                   references: docs/feat/app-ampsql.md docs/schemas/tables/evm-rpc.md\n  \
                   assets: docs/schemas/providers/evm-rpc.spec.json\n\
                 ---\n\
                 # Query {version}\n"
            ),
        ),
        RepoFile::new(
            "skills/amp-query/references/workflow-decoding.md",
            format!("decoding workflow {version}"),
        ),
        RepoFile::executable("skills/amp-query/scripts/check.sh", "#!/bin/sh\necho ok\n"),
        RepoFile::new("docs/feat/app-ampsql.md", format!("ampsql doc {version}")),
        RepoFile::new("docs/schemas/tables/evm-rpc.md", "evm-rpc tables"),
        RepoFile::new("docs/schemas/providers/evm-rpc.spec.json", "{}"),
        RepoFile::new("docs/feat/unrelated.md", "not linked by any skill"),
    ]
}

/// Build the gzipped tarball GitHub serves for a commit: every file sits under
/// a top-level `owner-repo-<short commit>` directory.
fn source_tarball(commit: &str, files: &[RepoFile]) -> Result<Vec<u8>> {
    let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::default()));

    for file in files {
        let mode = if file.is_executable { 0o755 } else { 0o644 };

        let mut header = Header::new_gnu();
        header.set_size(u64::try_from(file.contents.len())?);
        header.set_mode(mode);
        header.set_cksum();

        let entry_path = format!("edgeandnode-amp-{}/{}", &commit[..7], file.path);
        builder.append_data(&mut header, entry_path, file.contents.as_bytes())?;
    }

    Ok(builder.into_inner()?.finish()?)
}

/// A git ref the mock GitHub API knows: it resolves to `commit`, whose source
/// tarball holds `files`.
struct MockRef {
    git_ref: &'static str,
    commit: &'static str,
    files: Vec<RepoFile>,
    /// Pull request whose head is this ref, served by the pulls API.
    pull_request: Option<u32>,
}

impl MockRef {
    /// `v1.0.0` of the mock amp repository.
    fn v1() -> Self {
        Self {
            git_ref: "v1.0.0",
            commit: COMMIT_V1,
            files: amp_repo("v1.0.0"),
            pull_request: None,
        }
    }

    /// The head branch of pull request [`PR_NUMBER`] in the mock amp repository.
    fn pull_request() -> Self {
        Self {
            git_ref: "feature/new-skills",
            commit: COMMIT_PR,
            files: amp_repo("pull request"),
            pull_request: Some(PR_NUMBER),
        }
    }

    /// `v2.0.0` of the mock amp repository.
    fn v2() -> Self {
        Self {
            git_ref: "v2.0.0",
            commit: COMMIT_V2,
            files: amp_repo("v2.0.0"),
            pull_request: None,
        }
    }
}

/// Start a mock GitHub API serving the given refs, and return a client pointed
/// at it. Unknown refs, pull requests, and commits get a 404.
///
/// The server stops when the returned handle is dropped at the end of the test.
async fn start_mock_github(refs: Vec<MockRef>) -> Result<(GitHubClient, AbortOnDropHandle<()>)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    let mut routes = Vec::new();
    for mock_ref in refs {
        let tarball = source_tarball(mock_ref.commit, &mock_ref.files)?;
        routes.push(mock_github::Route::ok(
            format!("commits/{}", mock_ref.git_ref),
            mock_ref.commit.as_bytes().to_vec(),
        ));
        routes.push(mock_github::Route::ok(
            format!("tarball/{}", mock_ref.commit),
            tarball,
        ));
        if let Some(number) = mock_ref.pull_request {
            let pull_request_json = format!(r#"{{"head":{{"sha":"{}"}}}}"#, mock_ref.commit);
            routes.push(mock_github::Route::ok(
                format!("pulls/{}", number),
                pull_request_json.into_bytes(),
            ));
        }
    }
    let server = AbortOnDropHandle::new(mock_github::start(listener, routes));

    let github = GitHubClient::with_api_base(format!("http://{addr}"), None)?;
    Ok((github, server))
}

/// A mock amp installation plus an empty project directory to install skills
/// into.
struct TestEnv {
    amp_dir: TempInstallDir,
    project_dir: TempDir,
}

impl TestEnv {
    fn create() -> Result<Self> {
        Ok(Self {
            amp_dir: TempInstallDir::new()?,
            project_dir: TempDir::new()?,
        })
    }

    /// Mark an ampd version as active, as `ampup install` or `ampup use` would.
    fn set_active_version(&self, version: &str) -> Result<()> {
        fs::write(self.amp_dir.current_version_file(), version)?;
        Ok(())
    }

    fn config(&self) -> Result<Config> {
        Config::new(Some(self.amp_dir.path().to_path_buf()))
    }

    /// Install into the project, as `ampup skills` without `--global` does.
    fn project(&self) -> SkillsLocation {
        SkillsLocation {
            scope: Scope::Project,
            base_dir: self.project_dir.path().to_path_buf(),
        }
    }

    /// A path inside the project directory.
    fn project_path(&self, relative_path: &str) -> PathBuf {
        self.project_dir.path().join(relative_path)
    }
}

/// Parse skill names, as clap does for `--skill` flags.
fn skill_names(names: &[&str]) -> Result<Vec<SkillName>> {
    let mut skill_names = Vec::new();
    for name in names {
        skill_names.push(name.parse()?);
    }
    Ok(skill_names)
}

/// A selection of exactly the named skills, as `--skill` flags produce.
fn named(names: &[&str]) -> Result<SkillSelection> {
    Ok(SkillSelection::Named(skill_names(names)?))
}

/// Every file under `dir`, as sorted `/`-separated paths relative to `dir`.
fn files_under(dir: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let mut dirs_to_visit = vec![dir.to_path_buf()];

    while let Some(current_dir) = dirs_to_visit.pop() {
        for entry in fs::read_dir(&current_dir)? {
            let path = entry?.path();
            if path.is_dir() {
                dirs_to_visit.push(path);
            } else {
                let relative_path = path.strip_prefix(dir)?;
                files.push(relative_path.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    files.sort();
    Ok(files)
}

/// Names of the skills recorded in the lock file of `skills_dir`, sorted.
fn installed_names(skills_dir: &Path) -> Result<Vec<String>> {
    let lock = SkillsLock::load(skills_dir)?;
    Ok(lock.skills.keys().map(SkillName::to_string).collect())
}

fn installed_from(git_ref: &str, commit: &str) -> InstalledSkill {
    InstalledSkill {
        repo: MOCK_REPO.to_string(),
        git_ref: git_ref.to_string(),
        commit: commit.to_string(),
    }
}

/// Install skills at `v1.0.0` for Claude Code in the project, as a starting
/// point for list, update, and remove tests.
async fn install_v1_for_claude_code(
    env: &TestEnv,
    github: &GitHubClient,
    names: &[&str],
) -> Result<()> {
    skills::add_skills(
        github,
        &env.config()?,
        &env.project(),
        SourceRef::Git("v1.0.0".to_string()),
        named(names)?,
        vec![Agent::ClaudeCode],
    )
    .await?;
    Ok(())
}
