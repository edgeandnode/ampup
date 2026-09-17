//! The `ampup skills` commands: add, list, update, and remove agent skills.
//!
//! A skill ampup did not install (one missing from the lock file) is never
//! overwritten, updated, or removed. Each command writes its result to stdout
//! (as text, or as JSON with `--json`) and all status and warnings to stderr.

use std::{
    collections::BTreeSet,
    num::NonZeroU32,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use dialoguer::{Confirm, MultiSelect, theme::ColorfulTheme};

use crate::{
    DEFAULT_REPO,
    config::Config,
    github::GitHubClient,
    skills::{
        self, Agent, InstallSkillError, InstalledSkill, LoadSkillsError, LoadSkillsLockError,
        SaveSkillsLockError, Scope, Skill, SkillName, SkillsDir, SkillsLock, UninstallSkillError,
    },
    token, ui,
};

/// Deadline for resolving a git ref or pull request to a commit.
const RESOLVE_COMMIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Deadline for downloading the repository source tarball, which holds the
/// whole repository.
const SOURCE_TARBALL_TIMEOUT: Duration = Duration::from_secs(600);

/// Where skills are downloaded from, shared by the commands that download.
#[derive(Debug, clap::Args)]
pub struct SkillsSourceArgs {
    /// Git tag, branch, or commit to install from (defaults to the active ampd version)
    #[arg(long = "ref", value_name = "REF", conflicts_with = "pr")]
    pub git_ref: Option<String>,

    /// Pull request number to install from, at the current head of its branch
    #[arg(long, value_name = "NUMBER")]
    pub pr: Option<NonZeroU32>,

    /// Installation directory (defaults to $AMP_DIR or $XDG_CONFIG_HOME/.amp or $HOME/.amp)
    #[arg(long, env = "AMP_DIR")]
    pub install_dir: Option<PathBuf>,

    /// GitHub repository in format "owner/repo"
    #[arg(long, default_value_t = DEFAULT_REPO.to_string())]
    pub repo: String,

    /// GitHub token for private repository access (defaults to $GITHUB_TOKEN)
    #[arg(long, env = "GITHUB_TOKEN", hide_env = true)]
    pub github_token: Option<String>,
}

impl SkillsSourceArgs {
    /// The ref the `--ref` and `--pr` flags select, which clap keeps mutually
    /// exclusive; neither means the active ampd version.
    pub fn source_ref(&self) -> SourceRef {
        match (&self.git_ref, self.pr) {
            (Some(git_ref), _) => SourceRef::Git(git_ref.clone()),
            (None, Some(number)) => SourceRef::PullRequest(number),
            (None, None) => SourceRef::ActiveVersion,
        }
    }
}

/// The point in the repository history to download skills from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRef {
    /// The tag of the active ampd version, so skills match the installed binaries.
    ActiveVersion,
    /// A tag, branch, or commit.
    Git(String),
    /// The current head commit of a pull request.
    PullRequest(NonZeroU32),
}

/// Which skills a command acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSelection {
    /// Exactly these skills.
    Named(Vec<SkillName>),
    /// Every skill.
    All,
    /// Ask the user to pick.
    Prompt,
}

impl SkillSelection {
    /// The selection the `--skill` and `--all` flags describe: `--all` wins,
    /// then named skills, and neither means prompting.
    pub fn from_flags(skill_names: Vec<SkillName>, all: bool) -> Self {
        if all {
            Self::All
        } else if skill_names.is_empty() {
            Self::Prompt
        } else {
            Self::Named(skill_names)
        }
    }
}

/// List the skills available at a git ref, without installing them.
///
/// # Errors
///
/// Fails when the ampd config or GitHub client cannot be created, the skills
/// cannot be listed (see [`ListAvailableSkillsError`]), or the result cannot be
/// printed.
pub async fn list_available(source: SkillsSourceArgs, json: bool) -> Result<()> {
    let source_ref = source.source_ref();
    let config = Config::new(source.install_dir)?;
    let github = GitHubClient::new(
        source.repo,
        token::resolve_github_token(source.github_token),
    )?;

    let available = list_available_skills(&github, &config, source_ref).await?;
    print_result(&available, json)
}

/// Install skills.
///
/// # Errors
///
/// Fails when the ampd config, GitHub client, or install location cannot be
/// created, the skills cannot be installed (see [`AddSkillsError`]), or the
/// result cannot be printed.
pub async fn add(
    source: SkillsSourceArgs,
    selection: SkillSelection,
    agents: Vec<Agent>,
    scope: Scope,
    json: bool,
) -> Result<()> {
    let source_ref = source.source_ref();
    let config = Config::new(source.install_dir)?;
    let github = GitHubClient::new(
        source.repo,
        token::resolve_github_token(source.github_token),
    )?;
    let location = SkillsLocation::create(scope)?;

    let changes = add_skills(&github, &config, &location, source_ref, selection, agents).await?;
    print_result(&changes, json)
}

/// List the skills installed by ampup.
///
/// # Errors
///
/// Fails when the install location cannot be determined, a lock file cannot be
/// read (see [`ListInstalledSkillsError`]), or the result cannot be printed.
pub async fn list(agents: Vec<Agent>, scope: Scope, json: bool) -> Result<()> {
    let location = SkillsLocation::create(scope)?;

    // Reading lock files is blocking file I/O
    let list_task = tokio::task::spawn_blocking(move || list_installed_skills(&location, &agents));
    let installed = list_task.await.context("Skill listing task failed")??;

    if installed.dirs.is_empty() {
        ui::info_stderr!("No skills installed by ampup");
        ui::detail_stderr!("Run 'ampup skills add' to install skills");
    }
    print_result(&installed, json)
}

/// Reinstall installed skills from a git ref.
///
/// # Errors
///
/// Fails when the ampd config, GitHub client, or install location cannot be
/// created, the skills cannot be updated (see [`UpdateSkillsError`]), or the
/// result cannot be printed.
pub async fn update(
    source: SkillsSourceArgs,
    skill_names: Vec<SkillName>,
    agents: Vec<Agent>,
    scope: Scope,
    json: bool,
) -> Result<()> {
    let source_ref = source.source_ref();
    let config = Config::new(source.install_dir)?;
    let github = GitHubClient::new(
        source.repo,
        token::resolve_github_token(source.github_token),
    )?;
    let location = SkillsLocation::create(scope)?;

    let changes =
        update_skills(&github, &config, &location, source_ref, skill_names, agents).await?;
    print_result(&changes, json)
}

/// Remove skills installed by ampup, after confirming unless `force` is set.
///
/// # Errors
///
/// Fails when the install location cannot be determined, the skills cannot be
/// removed (see [`RemoveSkillsError`]), or the result cannot be printed.
pub async fn remove(
    selection: SkillSelection,
    agents: Vec<Agent>,
    scope: Scope,
    force: bool,
    json: bool,
) -> Result<()> {
    let location = SkillsLocation::create(scope)?;

    // Removing skills is blocking file I/O and may prompt the user
    let remove_task =
        tokio::task::spawn_blocking(move || remove_skills(&location, selection, &agents, force));
    let changes = remove_task.await.context("Skill removal task failed")??;

    print_result(&changes, json)
}

/// List the skills available at a git ref, using the given client and config.
///
/// # Errors
///
/// See [`ListAvailableSkillsError`].
// Split from `list_available` so tests can inject a client pointed at a mock server
pub(crate) async fn list_available_skills(
    github: &GitHubClient,
    config: &Config,
    source_ref: SourceRef,
) -> Result<AvailableSkills, ListAvailableSkillsError> {
    let resolved_ref = resolve_source_ref(config, source_ref)
        .map_err(ListAvailableSkillsError::ResolveSourceRef)?;
    let snapshot = download_snapshot(github, resolved_ref)
        .await
        .map_err(ListAvailableSkillsError::Download)?;

    // Decompressing and parsing the tarball is CPU-bound
    let load_task = tokio::task::spawn_blocking(move || {
        let skills = load_snapshot_skills(&snapshot)?;
        Ok(AvailableSkills {
            repo: snapshot.repo,
            git_ref: snapshot.git_ref,
            commit: snapshot.commit,
            skills: skills
                .into_iter()
                .map(|skill| AvailableSkill {
                    name: skill.name,
                    description: skill.description,
                })
                .collect(),
        })
    });

    load_task
        .await
        .map_err(ListAvailableSkillsError::LoadTask)?
        .map_err(ListAvailableSkillsError::LoadSkills)
}

/// Errors from listing the skills available at a git ref
#[derive(Debug, thiserror::Error)]
pub(crate) enum ListAvailableSkillsError {
    /// No ref could be determined
    #[error("Failed to determine the ref to list skills from")]
    ResolveSourceRef(#[source] ResolveSourceRefError),

    /// The repository snapshot could not be downloaded
    #[error("Failed to download skills")]
    Download(#[source] DownloadSnapshotError),

    /// The task reading the downloaded skills panicked or was cancelled
    #[error("Skill loading task failed")]
    LoadTask(#[source] tokio::task::JoinError),

    /// The downloaded snapshot has no readable skills
    #[error("Failed to load skills")]
    LoadSkills(#[source] LoadSnapshotSkillsError),
}

/// Install skills using the given client, config, and install location.
///
/// # Errors
///
/// See [`AddSkillsError`].
// Split from `add` so tests can inject a client pointed at a mock server
pub(crate) async fn add_skills(
    github: &GitHubClient,
    config: &Config,
    location: &SkillsLocation,
    source_ref: SourceRef,
    selection: SkillSelection,
    agents: Vec<Agent>,
) -> Result<SkillChanges, AddSkillsError> {
    let resolved_ref =
        resolve_source_ref(config, source_ref).map_err(AddSkillsError::ResolveSourceRef)?;
    let snapshot = download_snapshot(github, resolved_ref)
        .await
        .map_err(AddSkillsError::Download)?;

    // Parsing the tarball, prompting, and writing files all block
    let location = location.clone();
    let install_task = tokio::task::spawn_blocking(move || {
        install_from_snapshot(&snapshot, selection, agents, &location)
    });

    install_task
        .await
        .map_err(AddSkillsError::InstallTask)?
        .map_err(AddSkillsError::Install)
}

/// Errors from installing skills
#[derive(Debug, thiserror::Error)]
pub(crate) enum AddSkillsError {
    /// No ref could be determined
    #[error("Failed to determine the ref to install skills from")]
    ResolveSourceRef(#[source] ResolveSourceRefError),

    /// The repository snapshot could not be downloaded
    ///
    /// Nothing has been written when this occurs.
    #[error("Failed to download skills")]
    Download(#[source] DownloadSnapshotError),

    /// The task installing the skills panicked or was cancelled
    #[error("Skill install task failed")]
    InstallTask(#[source] tokio::task::JoinError),

    /// The skills could not be installed
    ///
    /// Skills installed before the failure stay installed and recorded in the
    /// lock file. Safe to retry: the next run replaces them.
    #[error("Failed to install skills")]
    Install(#[source] InstallFromSnapshotError),
}

/// List the skills installed by ampup at the given install location.
///
/// # Errors
///
/// See [`ListInstalledSkillsError`].
pub(crate) fn list_installed_skills(
    location: &SkillsLocation,
    agents: &[Agent],
) -> Result<InstalledSkills, ListInstalledSkillsError> {
    let agents = agents_or_all(agents);

    let mut dirs = Vec::new();
    for skills_dir in location.skills_dirs(&agents) {
        let lock = SkillsLock::load(&skills_dir.path).map_err(ListInstalledSkillsError)?;
        if lock.skills.is_empty() {
            continue;
        }

        dirs.push(InstalledSkillsDir {
            skills_dir: location.display(&skills_dir),
            agents: skills_dir.agents,
            skills: lock
                .skills
                .into_iter()
                .map(|(name, installed)| InstalledSkillEntry { name, installed })
                .collect(),
        });
    }

    Ok(InstalledSkills { dirs })
}

/// A skills lock file could not be read while listing installed skills
#[derive(Debug, thiserror::Error)]
#[error("Failed to read installed skills")]
pub(crate) struct ListInstalledSkillsError(#[source] LoadSkillsLockError);

/// Reinstall installed skills using the given client, config, and install
/// location.
///
/// # Errors
///
/// See [`UpdateSkillsError`].
// Split from `update` so tests can inject a client pointed at a mock server
pub(crate) async fn update_skills(
    github: &GitHubClient,
    config: &Config,
    location: &SkillsLocation,
    source_ref: SourceRef,
    skill_names: Vec<SkillName>,
    agents: Vec<Agent>,
) -> Result<SkillChanges, UpdateSkillsError> {
    let agents = agents_or_all(&agents);

    // Check for something to update before downloading anything
    let check_location = location.clone();
    let check_agents = agents.clone();
    let check_names = skill_names.clone();
    let check_task = tokio::task::spawn_blocking(move || {
        has_installed_skills(&check_location, &check_agents, &check_names)
    });
    let has_skills_to_update = check_task
        .await
        .map_err(UpdateSkillsError::CheckTask)?
        .map_err(UpdateSkillsError::CheckInstalled)?;
    if !has_skills_to_update {
        ui::info_stderr!("No installed skills to update");
        return Ok(SkillChanges::default());
    }

    let resolved_ref =
        resolve_source_ref(config, source_ref).map_err(UpdateSkillsError::ResolveSourceRef)?;
    let snapshot = download_snapshot(github, resolved_ref)
        .await
        .map_err(UpdateSkillsError::Download)?;

    // Parsing the tarball and writing files both block
    let location = location.clone();
    let update_task = tokio::task::spawn_blocking(move || {
        update_from_snapshot(&snapshot, &skill_names, &agents, &location)
    });

    update_task
        .await
        .map_err(UpdateSkillsError::UpdateTask)?
        .map_err(UpdateSkillsError::Update)
}

/// Errors from updating installed skills
#[derive(Debug, thiserror::Error)]
pub(crate) enum UpdateSkillsError {
    /// The task checking for installed skills panicked or was cancelled
    #[error("Installed skills check task failed")]
    CheckTask(#[source] tokio::task::JoinError),

    /// A skills lock file could not be read while checking for installed skills
    ///
    /// Nothing has been downloaded or written when this occurs.
    #[error("Failed to read installed skills")]
    CheckInstalled(#[source] LoadSkillsLockError),

    /// No ref could be determined
    #[error("Failed to determine the ref to update skills to")]
    ResolveSourceRef(#[source] ResolveSourceRefError),

    /// The repository snapshot could not be downloaded
    ///
    /// Nothing has been written when this occurs.
    #[error("Failed to download skills")]
    Download(#[source] DownloadSnapshotError),

    /// The task updating the skills panicked or was cancelled
    #[error("Skill update task failed")]
    UpdateTask(#[source] tokio::task::JoinError),

    /// The skills could not be updated
    ///
    /// Skills updated before the failure keep their new version. Safe to retry.
    #[error("Failed to update skills")]
    Update(#[source] UpdateFromSnapshotError),
}

/// Remove skills installed by ampup at the given install location, after
/// confirming unless `force` is set.
///
/// A declined confirmation removes nothing and returns no changes.
///
/// # Errors
///
/// See [`RemoveSkillsError`].
pub(crate) fn remove_skills(
    location: &SkillsLocation,
    selection: SkillSelection,
    agents: &[Agent],
    force: bool,
) -> Result<SkillChanges, RemoveSkillsError> {
    let agents = agents_or_all(agents);
    let skills_dirs = location.skills_dirs(&agents);

    let mut installed_names = BTreeSet::new();
    for skills_dir in &skills_dirs {
        let lock = SkillsLock::load(&skills_dir.path).map_err(RemoveSkillsError::LoadLock)?;
        installed_names.extend(lock.skills.into_keys());
    }
    if installed_names.is_empty() {
        ui::info_stderr!("No skills installed by ampup");
        return Ok(SkillChanges::default());
    }

    let installed_names: Vec<SkillName> = installed_names.into_iter().collect();
    let names_to_remove: Vec<SkillName> = match selection {
        SkillSelection::All => installed_names,
        SkillSelection::Named(skill_names) => {
            let mut names = Vec::new();
            for name in skill_names {
                if installed_names.contains(&name) {
                    names.push(name);
                } else {
                    ui::warn!("Skipping {}: not installed by ampup", name);
                }
            }
            names
        }
        SkillSelection::Prompt => {
            let items: Vec<String> = installed_names.iter().map(SkillName::to_string).collect();
            let selected_indexes = prompt_multi_select("Select skills to remove", &items)
                .map_err(RemoveSkillsError::SelectSkills)?;
            selected_indexes
                .into_iter()
                .map(|index| installed_names[index].clone())
                .collect()
        }
    };
    if names_to_remove.is_empty() {
        ui::info_stderr!("No skills selected");
        return Ok(SkillChanges::default());
    }

    if !force {
        let names: Vec<String> = names_to_remove.iter().map(SkillName::to_string).collect();
        let prompt = format!(
            "{} will no longer be available to your agents. Remove?",
            names.join(", ")
        );
        let is_confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .default(false)
            .interact()
            .map_err(RemoveSkillsError::Confirm)?;
        if !is_confirmed {
            ui::info_stderr!("Nothing removed");
            return Ok(SkillChanges::default());
        }
    }

    let mut changes = SkillChanges::default();
    for skills_dir in &skills_dirs {
        let mut lock = SkillsLock::load(&skills_dir.path).map_err(RemoveSkillsError::LoadLock)?;

        for name in &names_to_remove {
            if !lock.skills.contains_key(name) {
                continue;
            }

            skills::uninstall_skill(&skills_dir.path, name)
                .map_err(RemoveSkillsError::UninstallSkill)?;
            lock.skills.remove(name);
            lock.save(&skills_dir.path)
                .map_err(RemoveSkillsError::SaveLock)?;

            changes.changes.push(SkillChange {
                action: SkillAction::Removed,
                skill: name.clone(),
                skills_dir: location.display(skills_dir),
                agents: skills_dir.agents.clone(),
                source: None,
            });
        }
    }

    Ok(changes)
}

/// Errors from removing installed skills
///
/// Skills removed before a failure stay removed, and the lock file reflects
/// them. Safe to retry.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RemoveSkillsError {
    /// A skills lock file could not be read
    ///
    /// Occurs, among other causes, when the lock file names a skill that is not
    /// a valid directory name. Nothing is removed in that case.
    #[error("Failed to read installed skills")]
    LoadLock(#[source] LoadSkillsLockError),

    /// The prompt for picking skills failed
    ///
    /// Occurs when stdin is not a terminal; pass `--skill` or `--all` instead.
    #[error("Failed to get skill selection")]
    SelectSkills(#[source] dialoguer::Error),

    /// The confirmation prompt failed
    ///
    /// Occurs when stdin is not a terminal; pass `--force` to skip the prompt.
    #[error("Failed to get confirmation")]
    Confirm(#[source] dialoguer::Error),

    /// A skill could not be deleted
    #[error("Failed to remove skill")]
    UninstallSkill(#[source] UninstallSkillError),

    /// A lock file could not be updated after removing a skill
    #[error("Failed to update skills lock file")]
    SaveLock(#[source] SaveSkillsLockError),
}

/// The skills available at a git ref, as printed by `ampup skills add --list`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct AvailableSkills {
    /// GitHub repository in format `owner/repo`.
    pub repo: String,
    /// The ref the skills were read from.
    #[serde(rename = "ref")]
    pub git_ref: String,
    /// The commit the ref resolved to.
    pub commit: String,
    /// The skills, sorted by name.
    pub skills: Vec<AvailableSkill>,
}

/// One skill available at a git ref.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct AvailableSkill {
    pub name: SkillName,
    pub description: String,
}

impl std::fmt::Display for AvailableSkills {
    /// Formats each skill as its name followed by an indented description.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, skill) in self.skills.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(
                f,
                "{}\n  {}",
                ui::version(&skill.name),
                console::style(&skill.description).dim()
            )?;
        }
        Ok(())
    }
}

/// The skills ampup installed, grouped by skills directory, as printed by
/// `ampup skills list`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct InstalledSkills {
    /// Skills directories that hold at least one skill installed by ampup.
    pub dirs: Vec<InstalledSkillsDir>,
}

/// The skills ampup installed in one skills directory.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct InstalledSkillsDir {
    /// The directory as the user would type it (relative, or under `~`).
    pub skills_dir: String,
    /// The agents that read from this directory.
    pub agents: Vec<Agent>,
    /// The installed skills, sorted by name.
    pub skills: Vec<InstalledSkillEntry>,
}

/// One installed skill and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct InstalledSkillEntry {
    pub name: SkillName,
    #[serde(flatten)]
    pub installed: InstalledSkill,
}

impl std::fmt::Display for InstalledSkills {
    /// Formats each directory with its agents, then one indented line per
    /// skill with its ref and short commit.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, dir) in self.dirs.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(
                f,
                "{} ({})",
                ui::path(&dir.skills_dir),
                agent_names(&dir.agents)
            )?;
            for skill in &dir.skills {
                write!(
                    f,
                    "\n  {} {} ({})",
                    skill.name,
                    skill.installed.git_ref,
                    short_commit(&skill.installed.commit)
                )?;
            }
        }
        Ok(())
    }
}

/// What a command changed, as printed by `ampup skills add`, `update`, and
/// `remove`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub(crate) struct SkillChanges {
    /// One entry per skill per skills directory, in the order they happened.
    pub changes: Vec<SkillChange>,
}

/// A skill installed, updated, or removed in one skills directory.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct SkillChange {
    pub action: SkillAction,
    pub skill: SkillName,
    /// The directory as the user would type it (relative, or under `~`).
    pub skills_dir: String,
    /// The agents that read from this directory.
    pub agents: Vec<Agent>,
    /// Where the skill now comes from; absent for removals.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<InstalledSkill>,
}

/// The kind of change made to a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SkillAction {
    Installed,
    Updated,
    Removed,
}

impl std::fmt::Display for SkillChanges {
    /// Formats one checkmark line per change, such as
    /// `✓ Installed amp-query in .claude/skills (claude-code)`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, change) in self.changes.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            let verb = match change.action {
                SkillAction::Installed => "Installed",
                SkillAction::Updated => "Updated",
                SkillAction::Removed => "Removed",
            };
            let preposition = match change.action {
                SkillAction::Installed | SkillAction::Updated => "in",
                SkillAction::Removed => "from",
            };
            write!(
                f,
                "{} {} {} {} {} ({})",
                console::style("✓").green().bold(),
                verb,
                ui::version(&change.skill),
                preposition,
                ui::path(&change.skills_dir),
                agent_names(&change.agents)
            )?;
        }
        Ok(())
    }
}

/// Where skills commands install to: a scope and the directory it is relative
/// to (the project root, or the home directory for global skills).
// The CLI entry points derive it from the current and home directories; tests
// point it at a temporary directory
#[derive(Debug, Clone)]
pub(crate) struct SkillsLocation {
    pub scope: Scope,
    pub base_dir: PathBuf,
}

impl SkillsLocation {
    /// Project skills are relative to the current directory, global skills to
    /// the home directory.
    fn create(scope: Scope) -> Result<Self> {
        let base_dir = match scope {
            Scope::Global => {
                // A missing HOME is expected on Windows, where USERPROFILE holds
                // the home directory; the error that matters is both being missing
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .context("Could not determine home directory")?;
                home.into()
            }
            Scope::Project => {
                std::env::current_dir().context("Could not determine current directory")?
            }
        };

        Ok(Self { scope, base_dir })
    }

    fn skills_dirs(&self, agents: &[Agent]) -> Vec<SkillsDir> {
        skills::skills_dirs(agents, self.scope, &self.base_dir)
    }

    /// Show a skills directory the way the user would type it: relative to the
    /// project, or under `~` for global skills.
    fn display(&self, skills_dir: &SkillsDir) -> String {
        // Skills directories are always built from `base_dir`, so this only
        // falls back to the absolute path if that ever changes
        let Ok(relative_dir) = skills_dir.path.strip_prefix(&self.base_dir) else {
            return skills_dir.path.display().to_string();
        };

        match self.scope {
            Scope::Project => relative_dir.display().to_string(),
            Scope::Global => Path::new("~").join(relative_dir).display().to_string(),
        }
    }
}

/// The repository at one commit, downloaded but not yet parsed.
struct SourceSnapshot {
    repo: String,
    /// The ref as recorded in the lock file, such as `v0.1.0` or `pr/42`.
    git_ref: String,
    commit: String,
    /// The gzipped source tarball, in an anonymous temporary file that the
    /// operating system deletes once it is closed.
    tarball: std::fs::File,
}

/// Resolve a ref to a commit and download the repository at that commit.
async fn download_snapshot(
    github: &GitHubClient,
    resolved_ref: ResolvedRef,
) -> Result<SourceSnapshot, DownloadSnapshotError> {
    let git_ref = resolved_ref.label();
    ui::info_stderr!(
        "Fetching skills from {} at {}",
        github.repo(),
        ui::version(&git_ref)
    );

    // Download by commit, so the recorded commit is exactly what was installed
    // even if the ref moves in between
    let resolution = match &resolved_ref {
        ResolvedRef::Git(git_ref) => {
            tokio::time::timeout(RESOLVE_COMMIT_TIMEOUT, github.resolve_commit(git_ref)).await
        }
        ResolvedRef::PullRequest(number) => {
            tokio::time::timeout(
                RESOLVE_COMMIT_TIMEOUT,
                github.resolve_pull_request_head(*number),
            )
            .await
        }
    };
    let commit = match resolution {
        Ok(Ok(commit)) => commit,
        Ok(Err(err)) => return Err(DownloadSnapshotError::ResolveCommit(err)),
        // `Elapsed` carries nothing beyond the limit, which the variant records
        Err(_) => {
            return Err(DownloadSnapshotError::ResolveCommitTimeout(
                RESOLVE_COMMIT_TIMEOUT,
            ));
        }
    };

    let tarball = tempfile::tempfile().map_err(DownloadSnapshotError::CreateTarballFile)?;
    // The clone shares the underlying file, so what is written through it can be
    // read back from `tarball`
    let tarball_writer = tarball
        .try_clone()
        .map_err(DownloadSnapshotError::CreateTarballFile)?;
    let mut tarball_writer = tokio::fs::File::from_std(tarball_writer);

    let download = tokio::time::timeout(
        SOURCE_TARBALL_TIMEOUT,
        github.download_source_tarball(&commit, &mut tarball_writer),
    )
    .await;
    match download {
        Ok(Ok(())) => {}
        Ok(Err(err)) => return Err(DownloadSnapshotError::DownloadTarball(err)),
        // `Elapsed` carries nothing beyond the limit, which the variant records
        Err(_) => {
            return Err(DownloadSnapshotError::DownloadTarballTimeout(
                SOURCE_TARBALL_TIMEOUT,
            ));
        }
    }

    Ok(SourceSnapshot {
        repo: github.repo().to_string(),
        git_ref,
        commit,
        tarball,
    })
}

/// Errors from downloading a repository snapshot
#[derive(Debug, thiserror::Error)]
pub(crate) enum DownloadSnapshotError {
    /// The git ref or pull request could not be resolved to a commit
    ///
    /// Possible causes:
    /// - The ref or pull request does not exist
    /// - The repository is private and no GitHub token is available
    /// - GitHub rate limiting
    #[error("Failed to resolve the ref to a commit")]
    ResolveCommit(#[source] anyhow::Error),

    /// Resolving the git ref or pull request took longer than the deadline
    #[error("Resolving the ref timed out after {} seconds", .0.as_secs())]
    ResolveCommitTimeout(Duration),

    /// The temporary file for the source tarball could not be created
    ///
    /// Possible causes:
    /// - The temporary directory (`$TMPDIR`) is missing or not writable
    #[error("Failed to create a temporary file for the source tarball")]
    CreateTarballFile(#[source] std::io::Error),

    /// The source tarball could not be downloaded
    ///
    /// Possible causes:
    /// - Network failure or GitHub error
    /// - The temporary directory ran out of space
    #[error("Failed to download source tarball")]
    DownloadTarball(#[source] anyhow::Error),

    /// Downloading the source tarball took longer than the deadline
    #[error("Downloading the source tarball timed out after {} seconds", .0.as_secs())]
    DownloadTarballTimeout(Duration),
}

/// Read the skills of a downloaded snapshot, requiring at least one.
fn load_snapshot_skills(snapshot: &SourceSnapshot) -> Result<Vec<Skill>, LoadSnapshotSkillsError> {
    let skills =
        skills::load_skills(&snapshot.tarball).map_err(LoadSnapshotSkillsError::LoadSkills)?;

    if skills.is_empty() {
        return Err(LoadSnapshotSkillsError::NoSkillsInRef {
            repo: snapshot.repo.clone(),
            git_ref: snapshot.git_ref.clone(),
        });
    }

    Ok(skills)
}

/// Errors from reading the skills of a downloaded snapshot
#[derive(Debug, thiserror::Error)]
pub(crate) enum LoadSnapshotSkillsError {
    /// The skills in the snapshot could not be read
    #[error("Failed to read skills from the repository")]
    LoadSkills(#[source] LoadSkillsError),

    /// The snapshot has no skills
    ///
    /// Occurs for refs from before skills were added to the repository, such
    /// as older releases.
    #[error(
        "No skills found in {repo} at {git_ref}; try a newer ref with --ref <tag|branch|commit> or --pr <number>"
    )]
    NoSkillsInRef { repo: String, git_ref: String },
}

/// Select skills and agents, then install the skills into each skills
/// directory, recording each in the lock file first.
fn install_from_snapshot(
    snapshot: &SourceSnapshot,
    selection: SkillSelection,
    agents: Vec<Agent>,
    location: &SkillsLocation,
) -> Result<SkillChanges, InstallFromSnapshotError> {
    let available_skills =
        load_snapshot_skills(snapshot).map_err(InstallFromSnapshotError::LoadSkills)?;

    let selected_skills = select_skills_to_add(&available_skills, selection, snapshot)
        .map_err(InstallFromSnapshotError::SelectSkills)?;
    if selected_skills.is_empty() {
        ui::info_stderr!("No skills selected");
        return Ok(SkillChanges::default());
    }

    let agents = select_agents(agents).map_err(InstallFromSnapshotError::SelectAgents)?;
    if agents.is_empty() {
        ui::info_stderr!("No agents selected");
        return Ok(SkillChanges::default());
    }

    let installed = InstalledSkill {
        repo: snapshot.repo.clone(),
        git_ref: snapshot.git_ref.clone(),
        commit: snapshot.commit.clone(),
    };

    let mut changes = SkillChanges::default();
    for skills_dir in location.skills_dirs(&agents) {
        let mut lock =
            SkillsLock::load(&skills_dir.path).map_err(InstallFromSnapshotError::LoadLock)?;

        for skill in &selected_skills {
            // Never overwrite a skill the user wrote or installed another way
            let is_installed_by_ampup = lock.skills.contains_key(&skill.name);
            if skills_dir.path.join(skill.name.as_str()).exists() && !is_installed_by_ampup {
                ui::warn!(
                    "Skipping {}: it already exists in {} and was not installed by ampup",
                    skill.name,
                    ui::path(location.display(&skills_dir))
                );
                continue;
            }

            // Record first, so an interrupted install is retried rather than
            // skipped as a skill the user wrote
            lock.skills.insert(skill.name.clone(), installed.clone());
            lock.save(&skills_dir.path)
                .map_err(InstallFromSnapshotError::SaveLock)?;
            skills::install_skill(&skills_dir.path, skill)
                .map_err(InstallFromSnapshotError::InstallSkill)?;

            changes.changes.push(SkillChange {
                action: SkillAction::Installed,
                skill: skill.name.clone(),
                skills_dir: location.display(&skills_dir),
                agents: skills_dir.agents.clone(),
                source: Some(installed.clone()),
            });
        }
    }

    Ok(changes)
}

/// Errors from installing skills out of a downloaded snapshot
#[derive(Debug, thiserror::Error)]
pub(crate) enum InstallFromSnapshotError {
    /// The snapshot's skills could not be read
    ///
    /// Nothing has been written when this occurs.
    #[error("Failed to load skills")]
    LoadSkills(#[source] LoadSnapshotSkillsError),

    /// The requested skills could not be selected
    ///
    /// Nothing has been written when this occurs.
    #[error("Failed to select skills")]
    SelectSkills(#[source] SelectSkillsError),

    /// The prompt for picking agents failed
    ///
    /// Occurs when stdin is not a terminal; pass `--agent` instead.
    #[error("Failed to get agent selection")]
    SelectAgents(#[source] dialoguer::Error),

    /// A skills lock file could not be read
    #[error("Failed to read installed skills")]
    LoadLock(#[source] LoadSkillsLockError),

    /// A skills lock file could not be written before installing a skill
    #[error("Failed to update skills lock file")]
    SaveLock(#[source] SaveSkillsLockError),

    /// A skill could not be installed
    ///
    /// The skill is already recorded in the lock file, so running the command
    /// again replaces whatever was left behind.
    #[error("Failed to install skill")]
    InstallSkill(#[source] InstallSkillError),
}

/// Reinstall the requested installed skills from a downloaded snapshot.
fn update_from_snapshot(
    snapshot: &SourceSnapshot,
    skill_names: &[SkillName],
    agents: &[Agent],
    location: &SkillsLocation,
) -> Result<SkillChanges, UpdateFromSnapshotError> {
    let available_skills =
        load_snapshot_skills(snapshot).map_err(UpdateFromSnapshotError::LoadSkills)?;

    let installed = InstalledSkill {
        repo: snapshot.repo.clone(),
        git_ref: snapshot.git_ref.clone(),
        commit: snapshot.commit.clone(),
    };

    let mut changes = SkillChanges::default();
    for skills_dir in location.skills_dirs(agents) {
        let mut lock =
            SkillsLock::load(&skills_dir.path).map_err(UpdateFromSnapshotError::LoadLock)?;

        let installed_names: Vec<SkillName> = lock
            .skills
            .keys()
            .filter(|name| is_requested(name, skill_names))
            .cloned()
            .collect();

        for name in installed_names {
            let Some(skill) = available_skills.iter().find(|skill| skill.name == name) else {
                ui::warn!(
                    "Skipping {}: not found in {} at {}",
                    name,
                    snapshot.repo,
                    ui::version(&snapshot.git_ref)
                );
                continue;
            };

            // Record first, matching installs: a failed update is retried by
            // the next run
            lock.skills.insert(name.clone(), installed.clone());
            lock.save(&skills_dir.path)
                .map_err(UpdateFromSnapshotError::SaveLock)?;
            skills::install_skill(&skills_dir.path, skill)
                .map_err(UpdateFromSnapshotError::InstallSkill)?;

            changes.changes.push(SkillChange {
                action: SkillAction::Updated,
                skill: name,
                skills_dir: location.display(&skills_dir),
                agents: skills_dir.agents.clone(),
                source: Some(installed.clone()),
            });
        }
    }

    Ok(changes)
}

/// Errors from reinstalling skills out of a downloaded snapshot
#[derive(Debug, thiserror::Error)]
pub(crate) enum UpdateFromSnapshotError {
    /// The snapshot's skills could not be read
    ///
    /// Nothing has been written when this occurs.
    #[error("Failed to load skills")]
    LoadSkills(#[source] LoadSnapshotSkillsError),

    /// A skills lock file could not be read
    #[error("Failed to read installed skills")]
    LoadLock(#[source] LoadSkillsLockError),

    /// A skills lock file could not be written before updating a skill
    #[error("Failed to update skills lock file")]
    SaveLock(#[source] SaveSkillsLockError),

    /// A skill could not be reinstalled
    ///
    /// The lock file already records the new version, so running the command
    /// again replaces whatever was left behind.
    #[error("Failed to update skill")]
    InstallSkill(#[source] InstallSkillError),
}

/// Whether any of the requested skills is installed by ampup for the agents.
fn has_installed_skills(
    location: &SkillsLocation,
    agents: &[Agent],
    skill_names: &[SkillName],
) -> Result<bool, LoadSkillsLockError> {
    for skills_dir in location.skills_dirs(agents) {
        let lock = SkillsLock::load(&skills_dir.path)?;
        if lock
            .skills
            .keys()
            .any(|name| is_requested(name, skill_names))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Turn the requested ref into one that can be downloaded, using the active
/// ampd version when none was given so skills match the installed binaries.
fn resolve_source_ref(
    config: &Config,
    source_ref: SourceRef,
) -> Result<ResolvedRef, ResolveSourceRefError> {
    match source_ref {
        SourceRef::Git(git_ref) => Ok(ResolvedRef::Git(git_ref)),
        SourceRef::PullRequest(number) => Ok(ResolvedRef::PullRequest(number)),
        SourceRef::ActiveVersion => match config.current_version() {
            Ok(Some(version)) => Ok(ResolvedRef::Git(version)),
            Ok(None) => Err(ResolveSourceRefError::NoActiveVersion),
            Err(err) => Err(ResolveSourceRefError::ReadActiveVersion(err)),
        },
    }
}

/// Errors from determining the ref to download skills from
#[derive(Debug, thiserror::Error)]
pub(crate) enum ResolveSourceRefError {
    /// The active ampd version file could not be read
    #[error("Failed to read the active ampd version")]
    ReadActiveVersion(#[source] anyhow::Error),

    /// Neither `--ref` nor `--pr` was given and no ampd version is active
    ///
    /// Occurs before `ampup install` has run, or after the active version was
    /// uninstalled.
    #[error(
        "No active ampd version to match skills against; run 'ampup install' or pass --ref <tag|branch|commit> or --pr <number>"
    )]
    NoActiveVersion,
}

/// A ref that can be resolved to a commit on GitHub.
enum ResolvedRef {
    Git(String),
    PullRequest(NonZeroU32),
}

impl ResolvedRef {
    /// The ref as shown to the user and recorded in the lock file: the git ref
    /// itself, or `pr/<number>` for a pull request.
    fn label(&self) -> String {
        match self {
            Self::Git(git_ref) => git_ref.clone(),
            Self::PullRequest(number) => format!("pr/{}", number),
        }
    }
}

fn select_skills_to_add<'a>(
    available_skills: &'a [Skill],
    selection: SkillSelection,
    snapshot: &SourceSnapshot,
) -> Result<Vec<&'a Skill>, SelectSkillsError> {
    let skill_names = match selection {
        SkillSelection::All => return Ok(available_skills.iter().collect()),
        SkillSelection::Prompt => {
            let items: Vec<String> = available_skills
                .iter()
                .map(|skill| skill.name.to_string())
                .collect();
            let selected_indexes = prompt_multi_select("Select skills to install", &items)
                .map_err(SelectSkillsError::Prompt)?;
            return Ok(selected_indexes
                .into_iter()
                .map(|index| &available_skills[index])
                .collect());
        }
        SkillSelection::Named(skill_names) => skill_names,
    };

    let mut selected_skills = Vec::new();
    for name in skill_names {
        let Some(skill) = available_skills.iter().find(|skill| skill.name == name) else {
            return Err(SelectSkillsError::SkillNotFound {
                name,
                repo: snapshot.repo.clone(),
                git_ref: snapshot.git_ref.clone(),
                available: available_skills
                    .iter()
                    .map(|skill| skill.name.clone())
                    .collect(),
            });
        };
        selected_skills.push(skill);
    }

    Ok(selected_skills)
}

/// Errors from selecting the skills to install
#[derive(Debug, thiserror::Error)]
pub(crate) enum SelectSkillsError {
    /// The prompt for picking skills failed
    ///
    /// Occurs when stdin is not a terminal; pass `--skill` or `--all` instead.
    #[error("Failed to get skill selection")]
    Prompt(#[source] dialoguer::Error),

    /// A skill requested with `--skill` does not exist at the ref
    ///
    /// No skill is installed when this occurs, including valid ones requested
    /// alongside it.
    #[error(
        "Skill {name} not found in {repo} at {git_ref}; available skills: {}",
        skill_list(available)
    )]
    SkillNotFound {
        name: SkillName,
        repo: String,
        git_ref: String,
        available: Vec<SkillName>,
    },
}

/// Use the given agents, or ask the user to pick some.
fn select_agents(agents: Vec<Agent>) -> Result<Vec<Agent>, dialoguer::Error> {
    if !agents.is_empty() {
        return Ok(agents);
    }

    let items: Vec<String> = Agent::ALL
        .iter()
        .map(|agent| agent.name().to_string())
        .collect();
    let selected_indexes = prompt_multi_select("Select agents to install for", &items)?;

    Ok(selected_indexes
        .into_iter()
        .map(|index| Agent::ALL[index])
        .collect())
}

/// Commands that only act on installed skills default to every agent.
fn agents_or_all(agents: &[Agent]) -> Vec<Agent> {
    if agents.is_empty() {
        Agent::ALL.to_vec()
    } else {
        agents.to_vec()
    }
}

/// Ask the user to pick any number of items, and return the indexes they
/// picked.
fn prompt_multi_select(prompt: &str, items: &[String]) -> Result<Vec<usize>, dialoguer::Error> {
    MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("{} (space to toggle, enter to confirm)", prompt))
        .items(items)
        .interact()
}

/// Print a command's result to stdout, as JSON or as its human form.
fn print_result<T>(result: &T, json: bool) -> Result<()>
where
    T: serde::Serialize + std::fmt::Display,
{
    if json {
        let text = serde_json::to_string_pretty(result).context("Failed to serialize result")?;
        println!("{text}");
        return Ok(());
    }

    // An empty result has already been explained on stderr
    let text = result.to_string();
    if !text.is_empty() {
        println!("{text}");
    }
    Ok(())
}

fn agent_names(agents: &[Agent]) -> String {
    let names: Vec<&str> = agents.iter().map(|agent| agent.name()).collect();
    names.join(", ")
}

fn skill_list(names: &[SkillName]) -> String {
    let names: Vec<&str> = names.iter().map(SkillName::as_str).collect();
    names.join(", ")
}

/// An empty filter means every installed skill.
fn is_requested(name: &SkillName, skill_names: &[SkillName]) -> bool {
    skill_names.is_empty() || skill_names.contains(name)
}

fn short_commit(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;

    fn change(action: SkillAction, skill: &str) -> Result<SkillChange> {
        Ok(SkillChange {
            action,
            skill: skill.parse()?,
            skills_dir: ".claude/skills".to_string(),
            agents: vec![Agent::ClaudeCode],
            source: None,
        })
    }

    #[test]
    fn skill_changes_display_with_install_and_removal_renders_one_line_each() -> Result<()> {
        //* Given
        console::set_colors_enabled(false);
        let changes = SkillChanges {
            changes: vec![
                change(SkillAction::Installed, "amp-query")?,
                change(SkillAction::Removed, "amp-setup")?,
            ],
        };

        //* When
        let text = changes.to_string();

        //* Then
        assert_eq!(
            text,
            "✓ Installed amp-query in .claude/skills (claude-code)\n\
             ✓ Removed amp-setup from .claude/skills (claude-code)",
            "each change should render as one checkmark line"
        );

        Ok(())
    }

    #[test]
    fn skill_changes_display_with_no_changes_renders_nothing() {
        //* Given
        let changes = SkillChanges::default();

        //* When
        let text = changes.to_string();

        //* Then
        assert_eq!(text, "", "an empty result should print nothing to stdout");
    }

    #[test]
    fn skill_selection_from_flags_with_all_and_names_selects_all() -> Result<()> {
        //* Given
        let skill_names = vec!["amp-query".parse()?];

        //* When
        let selection = SkillSelection::from_flags(skill_names, true);

        //* Then
        assert_eq!(
            selection,
            SkillSelection::All,
            "--all should win over --skill"
        );

        Ok(())
    }
}
