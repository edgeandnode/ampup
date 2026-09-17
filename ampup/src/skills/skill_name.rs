//! The name of a skill, which is also the name of its directory.
//!
//! Skill names come from outside ampup (the repository tarball, the lock file,
//! and `--skill` flags) and are joined onto a skills directory before files are
//! written or deleted. A [`SkillName`] is always a single plain path component,
//! so it can never point outside the skills directory.

/// A validated skill name: a single path component, never `.`, `..`, or a path
/// with separators.
///
/// Parse one with [`str::parse`]; that is the only way to construct it.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct SkillName(String);

impl SkillName {
    /// The name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for SkillName {
    type Err = InvalidSkillNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let is_single_component =
            !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\']);

        if !is_single_component {
            return Err(InvalidSkillNameError {
                name: name.to_string(),
            });
        }

        Ok(Self(name.to_string()))
    }
}

impl TryFrom<String> for SkillName {
    type Error = InvalidSkillNameError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        name.parse()
    }
}

impl From<SkillName> for String {
    fn from(name: SkillName) -> Self {
        name.0
    }
}

impl std::fmt::Display for SkillName {
    /// Formats the name as written in the repository.
    ///
    /// ```
    /// let name: ampup::skills::SkillName = "amp-query".parse().unwrap();
    /// assert_eq!(name.to_string(), "amp-query");
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A string that is not a valid skill name
///
/// Occurs when a name is empty, `.`, `..`, or contains a path separator, so
/// using it as a directory name could reach outside the skills directory.
#[derive(Debug, thiserror::Error)]
#[error("Invalid skill name '{name}': a skill name must be a single directory name")]
pub struct InvalidSkillNameError {
    /// The rejected name.
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_with_plain_name_succeeds() {
        //* Given
        let name = "amp-query";

        //* When
        let result = name.parse::<SkillName>();

        //* Then
        assert_eq!(
            result
                .map(|skill_name| skill_name.to_string())
                .ok()
                .as_deref(),
            Some("amp-query"),
            "a plain directory name should be a valid skill name"
        );
    }

    #[test]
    fn from_str_with_parent_dir_fails() {
        //* Given
        let name = "..";

        //* When
        let result = name.parse::<SkillName>();

        //* Then
        assert!(
            result.is_err(),
            "'..' must be rejected so it cannot escape the skills directory"
        );
    }

    #[test]
    fn from_str_with_path_separator_fails() {
        //* Given
        let name = "../outside";

        //* When
        let result = name.parse::<SkillName>();

        //* Then
        assert!(
            result.is_err(),
            "a name with a separator must be rejected so it cannot escape the skills directory"
        );
    }
}
