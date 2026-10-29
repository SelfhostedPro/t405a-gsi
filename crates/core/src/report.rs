//! Progress reporting shared by the CLI and GUI.

/// A sink for human-readable progress. Implementations must be cheap and
/// thread-safe; the GUI forwards messages to its event loop.
pub trait Reporter: Send + Sync {
    fn log(&self, msg: &str);
    fn progress(&self, _done: u64, _total: Option<u64>) {}
}

/// Writes straight to stdout, for the CLI.
pub struct StdoutReporter;

impl Reporter for StdoutReporter {
    fn log(&self, msg: &str) {
        println!("{msg}");
    }
}
