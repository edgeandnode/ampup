//! The coding agents ampup installs skills for, and where each one reads them.

use std::path::{Path, PathBuf};

/// The directory an agent reads skills from in a scope.
///
/// Several agents can share a directory: Codex and Cursor both read project
/// skills from the agent-neutral `.agents/skills`.
///
/// `base_dir` is the project root for [`Scope::Project`], and the user's home
/// directory for [`Scope::Global`].
pub fn skills_dir(agent: Agent, scope: Scope, base_dir: &Path) -> PathBuf {
    let relative_dir = match (agent, scope) {
        (Agent::ClaudeCode, Scope::Project) => ".claude/skills",
        (Agent::ClaudeCode, Scope::Global) => ".claude/skills",
        (Agent::Codex, Scope::Project) => ".agents/skills",
        (Agent::Codex, Scope::Global) => ".codex/skills",
        (Agent::Cursor, Scope::Project) => ".agents/skills",
        (Agent::Cursor, Scope::Global) => ".cursor/skills",
    };
    base_dir.join(relative_dir)
}

/// A coding agent that ampup can install skills for.
///
/// Each agent reads skills from its own directory, both inside a project and in
/// the user's home directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Agent {
    /// Claude Code
    ClaudeCode,
    /// OpenAI Codex
    Codex,
    /// Cursor
    Cursor,
}

impl Agent {
    /// All supported agents, in the order they are listed to the user.
    pub const ALL: [Self; 3] = [Self::ClaudeCode, Self::Codex, Self::Cursor];

    /// The agent name as accepted by the `--agent` flag.
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
        }
    }
}

impl std::fmt::Display for Agent {
    /// Formats the agent as its `--agent` flag value.
    ///
    /// ```
    /// assert_eq!(ampup::skills::Agent::ClaudeCode.to_string(), "claude-code");
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Where skills are installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Inside the current project, shared with everyone working on it.
    Project,
    /// In the user's home directory, available in every project.
    Global,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_dir_with_codex_in_project_scope_uses_agents_dir() {
        //* Given
        let project_dir = Path::new("/work/project");

        //* When
        let dir = skills_dir(Agent::Codex, Scope::Project, project_dir);

        //* Then
        assert_eq!(
            dir,
            PathBuf::from("/work/project/.agents/skills"),
            "codex project skills should live in the agent-neutral .agents directory"
        );
    }

    #[test]
    fn skills_dir_with_codex_in_global_scope_uses_codex_dir() {
        //* Given
        let home_dir = Path::new("/home/user");

        //* When
        let dir = skills_dir(Agent::Codex, Scope::Global, home_dir);

        //* Then
        assert_eq!(
            dir,
            PathBuf::from("/home/user/.codex/skills"),
            "codex global skills should live in the .codex directory"
        );
    }
}
