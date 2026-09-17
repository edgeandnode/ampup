//! Parsing the YAML frontmatter of a `SKILL.md` file.

/// The parts of a `SKILL.md` frontmatter that ampup needs.
///
/// Parse one from a whole `SKILL.md` file with [`str::parse`]. The frontmatter
/// is the block between the opening `---` line and the next `---` line. See the
/// [Agent Skills specification](https://agentskills.io/specification).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    /// What the skill does and when to use it.
    pub description: String,
    /// Repository paths of files the skill links as `references/<file name>`.
    pub references: Vec<String>,
    /// Repository paths of files the skill links as `assets/<file name>`.
    pub assets: Vec<String>,
}

impl std::str::FromStr for Frontmatter {
    type Err = ParseFrontmatterError;

    fn from_str(skill_md: &str) -> Result<Self, Self::Err> {
        let mut lines = skill_md.lines();

        let Some(first_line) = lines.next() else {
            return Err(ParseFrontmatterError::Empty);
        };
        if first_line.trim_end() != "---" {
            return Err(ParseFrontmatterError::MissingOpeningDelimiter);
        }

        let mut yaml = String::new();
        let mut is_terminated = false;
        for line in lines {
            if line.trim_end() == "---" {
                is_terminated = true;
                break;
            }
            yaml.push_str(line);
            yaml.push('\n');
        }
        if !is_terminated {
            return Err(ParseFrontmatterError::MissingClosingDelimiter);
        }

        let raw: RawFrontmatter = match serde_saphyr::from_str(&yaml) {
            Ok(raw) => raw,
            Err(err) => return Err(ParseFrontmatterError::InvalidYaml(Box::new(err))),
        };

        Ok(Self {
            description: raw.description,
            references: split_paths(&raw.metadata.references),
            assets: split_paths(&raw.metadata.assets),
        })
    }
}

/// Errors from parsing a `SKILL.md` frontmatter block
#[derive(Debug, thiserror::Error)]
pub enum ParseFrontmatterError {
    /// The file has no content
    #[error("File is empty")]
    Empty,

    /// The file does not start with a `---` line
    ///
    /// Occurs for a `SKILL.md` without frontmatter, which the Agent Skills
    /// specification requires.
    #[error("File does not start with a '---' frontmatter block")]
    MissingOpeningDelimiter,

    /// The frontmatter block has no closing `---` line
    #[error("Frontmatter block is not closed with '---'")]
    MissingClosingDelimiter,

    /// The frontmatter block is not valid YAML, or a field has the wrong type
    ///
    /// Occurs, for example, when a `metadata` value is a list instead of the
    /// space-separated string the specification requires.
    // Boxed because the YAML error is large and would bloat every `Result`
    // carrying this error
    #[error("Frontmatter is not valid YAML")]
    InvalidYaml(#[source] Box<serde_saphyr::Error>),
}

#[derive(serde::Deserialize)]
struct RawFrontmatter {
    #[serde(default)]
    description: String,
    #[serde(default)]
    metadata: RawMetadata,
}

/// The specification requires `metadata` values to be strings, so each linked
/// file list is a single space-separated string.
#[derive(Default, serde::Deserialize)]
struct RawMetadata {
    #[serde(default)]
    references: String,
    #[serde(default)]
    assets: String,
}

fn split_paths(paths: &str) -> Vec<String> {
    paths.split_whitespace().map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_with_linked_files_returns_space_separated_paths()
    -> Result<(), ParseFrontmatterError> {
        //* Given
        let skill_md = "---\n\
            name: amp-providers\n\
            description: Configure providers.\n\
            metadata:\n  \
              references: docs/feat/provider.md docs/feat/provider-evm-rpc.md\n  \
              assets: docs/schemas/providers/evm-rpc.spec.json\n\
            ---\n\
            \n\
            # Providers\n";

        //* When
        let frontmatter = skill_md.parse::<Frontmatter>()?;

        //* Then
        assert_eq!(
            frontmatter,
            Frontmatter {
                description: "Configure providers.".to_string(),
                references: vec![
                    "docs/feat/provider.md".to_string(),
                    "docs/feat/provider-evm-rpc.md".to_string(),
                ],
                assets: vec!["docs/schemas/providers/evm-rpc.spec.json".to_string()],
            },
            "linked files should be split on whitespace"
        );

        Ok(())
    }

    #[test]
    fn from_str_without_metadata_returns_no_linked_files() -> Result<(), ParseFrontmatterError> {
        //* Given
        let skill_md = "---\nname: amp-system\ndescription: Inspect the system.\n---\n# System\n";

        //* When
        let frontmatter = skill_md.parse::<Frontmatter>()?;

        //* Then
        assert!(
            frontmatter.references.is_empty() && frontmatter.assets.is_empty(),
            "a skill without metadata should link no files"
        );

        Ok(())
    }

    #[test]
    fn from_str_without_frontmatter_block_fails() {
        //* Given
        let skill_md = "# Just a heading\n";

        //* When
        let result = skill_md.parse::<Frontmatter>();

        //* Then
        assert!(
            matches!(result, Err(ParseFrontmatterError::MissingOpeningDelimiter)),
            "a SKILL.md without frontmatter should be rejected"
        );
    }

    #[test]
    fn from_str_with_unclosed_frontmatter_block_fails() {
        //* Given
        let skill_md = "---\nname: amp-system\n";

        //* When
        let result = skill_md.parse::<Frontmatter>();

        //* Then
        assert!(
            matches!(result, Err(ParseFrontmatterError::MissingClosingDelimiter)),
            "an unclosed frontmatter block should be rejected"
        );
    }
}
