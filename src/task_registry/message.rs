/// The type of each output line produced by a managed child process.
pub type Output = String;

/// A message emitted by the managed process lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskMessage {
    /// A single line of stdout data from the child process.
    Data(Output),
    /// An error or diagnostic string (e.g., spawn failure, read error).
    Error(Output),
    /// The process has exited and its stdout pipe has been closed.
    Eof,
}
