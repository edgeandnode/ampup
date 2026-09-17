---
name: "app-ampup-skills"
description: "ampup skills add, list, update, and remove for installing amp agent skills into Claude Code, Codex, and Cursor skill directories, with linked references and assets. Load when asking about agent skills, installing skills, skill versions, or the .amp-skills.json lock file"
type: "feature"
status: "experimental"
components: "app:ampup"
---

# ampup skills - Agent Skills Installer

## Summary

`ampup skills` installs the agent skills that live in the amp repository's `skills/` directory into the skill directories of coding agents, in the current project or in the home directory. Skills are not published as release assets, so ampup reads them from the repository source at a git ref, which defaults to the active ampd version so skills match the installed binaries. Each skill's `SKILL.md` frontmatter can link feature docs and schemas from elsewhere in the repository; ampup copies those into the skill's `references/` and `assets/` directories so the installed skill is self-contained, following the [Agent Skills specification](https://agentskills.io/specification).

## Table of Contents

1. [Key Concepts](#key-concepts)
2. [Architecture](#architecture)
3. [Configuration](#configuration)
4. [Usage](#usage)
5. [Implementation](#implementation)
6. [Limitations](#limitations)
7. [References](#references)

## Key Concepts

- **Skill**: A `skills/<name>/` directory in the amp repository that contains a `SKILL.md`
- **Linked files**: Repository paths listed in the `SKILL.md` frontmatter `metadata`, as space-separated strings. Paths under `references` are installed as `references/<file name>`, paths under `assets` as `assets/<file name>`
- **Git ref**: The tag, branch, or commit skills are installed from (`--ref`), or a pull request (`--pr <number>`) at the current head of its branch, which also works for pull requests from forks. Defaults to the active ampd version (the `.version` file). A pull request is recorded as `pr/<number>`
- **Scope**: `project` installs relative to the current directory; `global` (`--global`) installs relative to the home directory
- **Lock file**: `.amp-skills.json` in each skills directory, recording the repository, ref, and commit of every skill ampup installed there. Skills missing from it are never updated or removed by ampup
- **Skill name**: A skill directory name, always a single path component. Names from the tarball, the lock file, and `--skill` that are empty, `.`, `..`, or contain a path separator are rejected, so no operation can reach outside the skills directory

## Architecture

### Agent Skill Directories

| Agent | `--agent` | Project | Global |
|-------|-----------|---------|--------|
| Claude Code | `claude-code` | `.claude/skills/` | `~/.claude/skills/` |
| Codex | `codex` | `.agents/skills/` | `~/.codex/skills/` |
| Cursor | `cursor` | `.agents/skills/` | `~/.cursor/skills/` |

Agents that share a directory (Codex and Cursor in a project) are installed into once.

### Install Flow

1. User runs `ampup skills add [--ref <ref>]`
2. Resolve the ref: `--ref`, `--pr`, or the active ampd version
3. Resolve GitHub token (explicit `--github-token` → `gh auth token` → unauthenticated)
4. Resolve the ref to a commit (a pull request resolves to its head commit), then stream the repository source tarball of that commit to an anonymous temporary file
5. Read every `skills/<name>/` directory with a `SKILL.md`, parse its frontmatter, and copy linked files into `references/` and `assets/`. A linked path that does not exist fails the install
6. Select skills (`--skill`, `--all`, or an interactive prompt) and agents (`--agent` or an interactive prompt)
7. For each skills directory, record the skill's repository, ref, and commit in `.amp-skills.json` first, so an interrupted install is retried rather than skipped on the next run. Then write each skill to a hidden staging directory, then move it into `<skills dir>/<name>`, replacing a previous copy installed by ampup. An existing skill directory that ampup did not install is skipped with a warning

### Communication

```
ampup → GitHub API (commits)   # Resolve the git ref to a commit
ampup → GitHub API (pulls)     # Resolve a pull request to its head commit
ampup → GitHub API (tarball)   # Download the repository source
```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `AMP_DIR` | `$XDG_CONFIG_HOME/.amp` or `$HOME/.amp` | Base installation directory, read for the active ampd version |
| `GITHUB_TOKEN` | (none) | GitHub token for private repository access and API rate limits |

## Usage

### Install Skills

```bash
# Pick skills and agents interactively, from the active ampd version
ampup skills add

# List the available skills without installing
ampup skills add --list

# Install specific skills for Claude Code
ampup skills add --skill amp-setup --skill amp-query --agent claude-code

# Install every skill for all agents from a specific ref
ampup skills add --all -a claude-code -a codex -a cursor --ref v0.1.0

# Try the skills of a pull request before it merges
ampup skills add --all --agent claude-code --pr 3087

# Install in the home directory
ampup skills add --all --agent claude-code --global

# Print what was installed as JSON
ampup skills add --all --agent claude-code --json
```

### List Installed Skills

```bash
# Skills installed by ampup in the current project
ampup skills list

# Skills installed by ampup in the home directory, for one agent
ampup skills list --global --agent codex

# As JSON
ampup skills list --json
```

Shows each skills directory with its agents, and every installed skill with its ref and commit.

### Update Skills

```bash
# Reinstall every installed skill from the active ampd version
ampup skills update

# Update one skill to a branch
ampup skills update --skill amp-query --ref main

# Update every installed skill to the latest head of a pull request
ampup skills update --pr 3087
```

Skills that no longer exist at the ref are skipped with a warning.

### Remove Skills

```bash
# Pick installed skills to remove interactively
ampup skills remove

# Remove specific skills
ampup skills remove --skill amp-query

# Remove every skill installed by ampup, for Cursor only, without confirming
ampup skills remove --all --agent cursor --force
```

`remove` asks for confirmation unless `--force` is given; declining removes nothing and exits successfully. `list`, `update`, and `remove` act on all agents unless `--agent` is given.

### Output

Every `skills` command prints its result, and only its result, to stdout: text by default, or JSON with `--json` (`add --list` prints the available skills, `list` the installed skills grouped by directory, and `add`, `update`, and `remove` the changes they made). Progress, status, and warnings go to stderr, so `--json` output can be piped. Errors print the failing step followed by its causes.

## Implementation

### Source Files

- `ampup/src/commands/skills.rs` - `add`, `list`, `update`, and `remove` commands, including ref defaulting, interactive selection and confirmation, download deadlines, and text and JSON results
- `ampup/src/skills.rs` - Loading skills from a source tarball, copying linked files, installing and removing skill directories
- `ampup/src/skills/agent.rs` - Supported agents and their project and global skill directories
- `ampup/src/skills/archive.rs` - Reading selected files from a GitHub source tarball
- `ampup/src/skills/frontmatter.rs` - `SKILL.md` frontmatter parsing
- `ampup/src/skills/lock.rs` - `.amp-skills.json` lock file
- `ampup/src/skills/skill_name.rs` - Validated skill names
- `ampup/src/github.rs` - Git ref resolution and source tarball download
- `ampup/src/tests/it_skills.rs` - End-to-end tests of listing, adding, updating, and removing skills against a mock GitHub API

## Limitations

- The default ref is the active ampd version, so it fails for refs that do not contain `skills/` (releases published before skills were added) and for custom build names; use `--ref`
- Replacing a skill removes the previous copy before moving the new one in, so a failed move leaves the skill uninstalled until the command is re-run
- `update` without `--ref` or `--pr` uses the active ampd version, not the ref each skill was installed from, so skills installed from a branch or pull request move to the release unless the ref is passed again
- The whole repository tarball is downloaded to read the skills, streamed to a temporary file rather than held in memory; resolving the ref times out after 30 seconds and the download after 10 minutes

## References

- [app-ampup](app-ampup.md) - Base: ampup version manager and installer
