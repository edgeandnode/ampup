use console::style;

/// Print a success message with a green checkmark
macro_rules! success {
    ($($arg:tt)*) => {
        println!("{} {}", console::style("✓").green().bold(), format!($($arg)*))
    };
}

/// Print an info message with a cyan arrow
macro_rules! info {
    ($($arg:tt)*) => {
        println!("{} {}", console::style("→").cyan(), format!($($arg)*))
    };
}

/// Print a warning message with a yellow warning symbol
macro_rules! warning {
    ($($arg:tt)*) => {
        eprintln!("{} {}", console::style("⚠").yellow().bold(), format!($($arg)*))
    };
}

/// Print a dimmed detail message (indented)
macro_rules! detail {
    ($($arg:tt)*) => {
        println!("  {}", console::style(format!($($arg)*)).dim())
    };
}

/// Print an info message with a cyan arrow to stderr
///
/// For commands whose stdout carries only their result, such as `--json` output.
macro_rules! info_stderr {
    ($($arg:tt)*) => {
        eprintln!("{} {}", console::style("→").cyan(), format!($($arg)*))
    };
}

/// Print a dimmed detail message (indented) to stderr
///
/// For commands whose stdout carries only their result, such as `--json` output.
macro_rules! detail_stderr {
    ($($arg:tt)*) => {
        eprintln!("  {}", console::style(format!($($arg)*)).dim())
    };
}

pub(crate) use detail;
pub(crate) use detail_stderr;
pub(crate) use info;
pub(crate) use info_stderr;
pub(crate) use success;
pub(crate) use warning as warn;

/// Style a version string (bold white)
pub fn version(v: impl std::fmt::Display) -> String {
    style(v).bold().to_string()
}

/// Style a path (cyan)
pub fn path(p: impl std::fmt::Display) -> String {
    style(p).cyan().to_string()
}
