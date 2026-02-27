use std::sync::{Arc, Mutex};

use console::{Term, style};

// ---------------------------------------------------------------------------
// Public trait
// ---------------------------------------------------------------------------

/// Reports aggregate download progress to the user.
///
/// Implementations handle TTY vs. non-TTY output formatting.
/// Shared across concurrent tasks via `Arc<dyn ProgressReporter>`.
pub trait ProgressReporter: Send + Sync {
    /// Register the components that will be downloaded.
    ///
    /// Called once before any tasks start.
    fn set_total(&self, total: usize, names: Vec<String>);

    /// Mark a component as actively downloading.
    ///
    /// Called from inside the spawned task after the semaphore permit
    /// is acquired, indicating the download has begun.
    fn component_started(&self, name: &str);

    /// Mark a component as successfully downloaded.
    ///
    /// Called from the result collection loop after a task completes.
    fn component_completed(&self, name: &str);

    /// Mark a component as failed.
    ///
    /// Called from the result collection loop when a task returns an error.
    fn component_failed(&self, name: &str);

    /// Finalize the progress display.
    ///
    /// Called after all tasks have completed (or after fail-fast shutdown).
    fn finish(&self);
}

/// Create a progress reporter appropriate for the current terminal.
///
/// Returns [`TtyProgress`] when stderr is a TTY (interactive terminal),
/// [`CiProgress`] otherwise (piped output, CI environments).
pub fn create_reporter() -> Arc<dyn ProgressReporter> {
    let term = Term::stderr();
    if term.is_term() {
        Arc::new(TtyProgress::new(term))
    } else {
        Arc::new(CiProgress::new())
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ComponentStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
}

struct ProgressState {
    names: Vec<String>,
    statuses: Vec<ComponentStatus>,
    completed_count: usize,
}

impl ProgressState {
    fn new() -> Self {
        Self {
            names: Vec::new(),
            statuses: Vec::new(),
            completed_count: 0,
        }
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }
}

// ---------------------------------------------------------------------------
// TTY progress reporter
// ---------------------------------------------------------------------------

/// Interactive progress reporter using in-place terminal line updates.
///
/// Renders a status line per component, updating in place as downloads
/// progress. Uses stderr so stdout remains clean for piping.
struct TtyProgress {
    term: Term,
    state: Mutex<ProgressState>,
    lines_drawn: Mutex<usize>,
}

impl TtyProgress {
    fn new(term: Term) -> Self {
        Self {
            term,
            state: Mutex::new(ProgressState::new()),
            lines_drawn: Mutex::new(0),
        }
    }

    /// Redraw all component status lines in place.
    ///
    /// Terminal write failures are best-effort — progress display failure
    /// should not abort downloads.
    fn redraw(&self, state: &ProgressState) {
        let mut lines_drawn = self.lines_drawn.lock().expect("lines_drawn lock poisoned");

        // Move cursor back to overwrite previous output
        if *lines_drawn > 0 {
            let _ = self.term.clear_last_lines(*lines_drawn);
        }

        if state.names.is_empty() {
            *lines_drawn = 0;
            return;
        }

        let max_name_len = state.names.iter().map(|n| n.len()).max().unwrap_or(0);

        for (i, name) in state.names.iter().enumerate() {
            let status = state.statuses[i];
            let line = format_tty_line(name, max_name_len, status);
            let _ = self.term.write_line(&line);
        }

        *lines_drawn = state.names.len();
    }
}

impl ProgressReporter for TtyProgress {
    fn set_total(&self, _total: usize, names: Vec<String>) {
        let mut state = self.state.lock().expect("state lock poisoned");
        let count = names.len();
        state.statuses = vec![ComponentStatus::Pending; count];
        state.names = names;
        self.redraw(&state);
    }

    fn component_started(&self, name: &str) {
        let mut state = self.state.lock().expect("state lock poisoned");
        if let Some(idx) = state.index_of(name) {
            state.statuses[idx] = ComponentStatus::Downloading;
        }
        self.redraw(&state);
    }

    fn component_completed(&self, name: &str) {
        let mut state = self.state.lock().expect("state lock poisoned");
        if let Some(idx) = state.index_of(name) {
            state.statuses[idx] = ComponentStatus::Completed;
            state.completed_count += 1;
        }
        self.redraw(&state);
    }

    fn component_failed(&self, name: &str) {
        let mut state = self.state.lock().expect("state lock poisoned");
        if let Some(idx) = state.index_of(name) {
            state.statuses[idx] = ComponentStatus::Failed;
        }
        self.redraw(&state);
    }

    fn finish(&self) {
        // Final redraw to ensure terminal is in a clean state
        let state = self.state.lock().expect("state lock poisoned");
        self.redraw(&state);
    }
}

// ---------------------------------------------------------------------------
// CI progress reporter
// ---------------------------------------------------------------------------

/// Append-only progress reporter for non-interactive environments.
///
/// Prints one line per component completion to avoid garbled output
/// in CI pipelines and piped contexts.
struct CiProgress {
    state: Mutex<ProgressState>,
}

impl CiProgress {
    fn new() -> Self {
        Self {
            state: Mutex::new(ProgressState::new()),
        }
    }
}

impl ProgressReporter for CiProgress {
    fn set_total(&self, _total: usize, names: Vec<String>) {
        let mut state = self.state.lock().expect("state lock poisoned");
        let count = names.len();
        state.statuses = vec![ComponentStatus::Pending; count];
        state.names = names;
    }

    fn component_started(&self, _name: &str) {
        // No output for CI — only report completions
    }

    fn component_completed(&self, name: &str) {
        let mut state = self.state.lock().expect("state lock poisoned");
        if let Some(idx) = state.index_of(name) {
            state.statuses[idx] = ComponentStatus::Completed;
            state.completed_count += 1;
        }
        let total = state.names.len();
        let completed = state.completed_count;
        println!(
            "  {} [{}/{}] Downloaded {}",
            style("✓").green().bold(),
            completed,
            total,
            name
        );
    }

    fn component_failed(&self, name: &str) {
        let mut state = self.state.lock().expect("state lock poisoned");
        if let Some(idx) = state.index_of(name) {
            state.statuses[idx] = ComponentStatus::Failed;
        }
        let total = state.names.len();
        let completed = state.completed_count;
        eprintln!(
            "  {} [{}/{}] Failed {}",
            style("✗").red().bold(),
            completed,
            total,
            name
        );
    }

    fn finish(&self) {
        // No cleanup needed for append-only output
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn format_tty_line(name: &str, max_name_len: usize, status: ComponentStatus) -> String {
    let padded_name = format!("{:width$}", name, width = max_name_len);
    match status {
        ComponentStatus::Pending => {
            format!("  {}   {}", padded_name, style("waiting...").dim())
        }
        ComponentStatus::Downloading => {
            format!("  {}   {} downloading...", padded_name, style("→").cyan())
        }
        ComponentStatus::Completed => {
            format!(
                "  {}   {} downloaded",
                padded_name,
                style("✓").green().bold()
            )
        }
        ComponentStatus::Failed => {
            format!("  {}   {} failed", padded_name, style("✗").red().bold())
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    mod progress_state {
        use super::*;

        #[test]
        fn index_of_with_existing_name_returns_index() {
            //* Given
            let mut state = ProgressState::new();
            state.names = vec!["ampd".to_string(), "ampctl".to_string()];
            state.statuses = vec![ComponentStatus::Pending; 2];

            //* When
            let result = state.index_of("ampctl");

            //* Then
            assert_eq!(result, Some(1), "should return index of matching name");
        }

        #[test]
        fn index_of_with_missing_name_returns_none() {
            //* Given
            let mut state = ProgressState::new();
            state.names = vec!["ampd".to_string()];
            state.statuses = vec![ComponentStatus::Pending];

            //* When
            let result = state.index_of("nonexistent");

            //* Then
            assert_eq!(result, None, "should return None for missing name");
        }
    }

    mod tty_progress {
        use super::*;

        /// Helper to create a TtyProgress with a buffered terminal for testing.
        fn test_reporter() -> TtyProgress {
            TtyProgress::new(Term::buffered_stderr())
        }

        #[test]
        fn set_total_with_names_initializes_all_as_pending() {
            //* Given
            let reporter = test_reporter();

            //* When
            reporter.set_total(2, vec!["ampd".to_string(), "ampctl".to_string()]);

            //* Then
            let state = reporter.state.lock().expect("lock");
            assert_eq!(state.names.len(), 2);
            assert!(
                state
                    .statuses
                    .iter()
                    .all(|s| *s == ComponentStatus::Pending),
                "all components should start as Pending"
            );
        }

        #[test]
        fn component_started_with_valid_name_sets_downloading() {
            //* Given
            let reporter = test_reporter();
            reporter.set_total(2, vec!["ampd".to_string(), "ampctl".to_string()]);

            //* When
            reporter.component_started("ampd");

            //* Then
            let state = reporter.state.lock().expect("lock");
            assert_eq!(
                state.statuses[0],
                ComponentStatus::Downloading,
                "started component should be Downloading"
            );
            assert_eq!(
                state.statuses[1],
                ComponentStatus::Pending,
                "other component should remain Pending"
            );
        }

        #[test]
        fn component_completed_with_valid_name_sets_completed_and_increments_count() {
            //* Given
            let reporter = test_reporter();
            reporter.set_total(2, vec!["ampd".to_string(), "ampctl".to_string()]);
            reporter.component_started("ampd");

            //* When
            reporter.component_completed("ampd");

            //* Then
            let state = reporter.state.lock().expect("lock");
            assert_eq!(
                state.statuses[0],
                ComponentStatus::Completed,
                "completed component should be Completed"
            );
            assert_eq!(
                state.completed_count, 1,
                "completed count should be incremented"
            );
        }

        #[test]
        fn component_failed_with_valid_name_sets_failed() {
            //* Given
            let reporter = test_reporter();
            reporter.set_total(1, vec!["ampd".to_string()]);
            reporter.component_started("ampd");

            //* When
            reporter.component_failed("ampd");

            //* Then
            let state = reporter.state.lock().expect("lock");
            assert_eq!(
                state.statuses[0],
                ComponentStatus::Failed,
                "failed component should be Failed"
            );
        }

        #[test]
        fn component_started_with_unknown_name_does_not_panic() {
            //* Given
            let reporter = test_reporter();
            reporter.set_total(1, vec!["ampd".to_string()]);

            //* When / Then — should not panic
            reporter.component_started("nonexistent");

            let state = reporter.state.lock().expect("lock");
            assert_eq!(
                state.statuses[0],
                ComponentStatus::Pending,
                "existing component should be unchanged"
            );
        }

        #[test]
        fn set_total_with_zero_components_does_not_panic() {
            //* Given
            let reporter = test_reporter();

            //* When / Then — should not panic
            reporter.set_total(0, vec![]);
            reporter.finish();
        }
    }

    mod ci_progress {
        use super::*;

        #[test]
        fn component_completed_with_valid_name_increments_count() {
            //* Given
            let reporter = CiProgress::new();
            reporter.set_total(2, vec!["ampd".to_string(), "ampctl".to_string()]);

            //* When
            reporter.component_completed("ampd");

            //* Then
            let state = reporter.state.lock().expect("lock");
            assert_eq!(
                state.completed_count, 1,
                "completed count should be incremented"
            );
            assert_eq!(
                state.statuses[0],
                ComponentStatus::Completed,
                "component should be marked Completed"
            );
        }

        #[test]
        fn component_failed_with_valid_name_sets_failed() {
            //* Given
            let reporter = CiProgress::new();
            reporter.set_total(1, vec!["ampd".to_string()]);

            //* When
            reporter.component_failed("ampd");

            //* Then
            let state = reporter.state.lock().expect("lock");
            assert_eq!(
                state.statuses[0],
                ComponentStatus::Failed,
                "component should be marked Failed"
            );
        }

        #[test]
        fn component_started_does_not_change_state() {
            //* Given
            let reporter = CiProgress::new();
            reporter.set_total(1, vec!["ampd".to_string()]);

            //* When
            reporter.component_started("ampd");

            //* Then
            let state = reporter.state.lock().expect("lock");
            assert_eq!(
                state.statuses[0],
                ComponentStatus::Pending,
                "CI reporter should not change state on started"
            );
        }
    }

    mod format_tty_line {
        use super::*;

        #[test]
        fn format_tty_line_with_pending_status_contains_waiting() {
            //* Given / When
            let line = format_tty_line("ampd", 10, ComponentStatus::Pending);

            //* Then
            assert!(
                line.contains("waiting..."),
                "pending line should contain 'waiting...', got: {}",
                line
            );
        }

        #[test]
        fn format_tty_line_with_downloading_status_contains_downloading() {
            //* Given / When
            let line = format_tty_line("ampd", 10, ComponentStatus::Downloading);

            //* Then
            assert!(
                line.contains("downloading..."),
                "downloading line should contain 'downloading...', got: {}",
                line
            );
        }

        #[test]
        fn format_tty_line_with_completed_status_contains_downloaded() {
            //* Given / When
            let line = format_tty_line("ampd", 10, ComponentStatus::Completed);

            //* Then
            assert!(
                line.contains("downloaded"),
                "completed line should contain 'downloaded', got: {}",
                line
            );
        }

        #[test]
        fn format_tty_line_with_failed_status_contains_failed() {
            //* Given / When
            let line = format_tty_line("ampd", 10, ComponentStatus::Failed);

            //* Then
            assert!(
                line.contains("failed"),
                "failed line should contain 'failed', got: {}",
                line
            );
        }
    }

    mod create_reporter {
        use super::*;

        #[test]
        fn create_reporter_returns_reporter_without_panic() {
            //* Given / When
            let reporter = create_reporter();

            //* Then — smoke test: call methods without panic
            reporter.set_total(1, vec!["test".to_string()]);
            reporter.component_started("test");
            reporter.component_completed("test");
            reporter.finish();
        }
    }
}
