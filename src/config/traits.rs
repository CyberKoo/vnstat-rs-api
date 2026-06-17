use anyhow::Result;

/// Common interface for configuration types that require post-deserialization
/// processing.
///
/// Implementors provide hooks for:
/// - **Finalization** — normalizing or transforming raw parsed values (e.g.
///   resolving hostname aliases into IP addresses) before validation.
/// - **Validation** — checking that the finalised values are semantically
///   correct (e.g. confirming a file path exists).
///
/// Both methods have default no-op implementations so that implementors
/// only need to override the steps they care about.
pub trait ConfigEntity {
    /// Post-processes the configuration after deserialization.
    ///
    /// Use this method to normalize aliases, resolve relative paths, or apply
    /// any other transformations that are needed before validation.
    ///
    /// # Errors
    ///
    /// Return an error if finalization cannot complete (e.g. an unresolvable
    /// hostname alias).
    fn finalize(&mut self) -> Result<()> {
        Ok(())
    }

    /// Validates the configuration after finalization.
    ///
    /// Checks that all values are semantically valid and internally
    /// consistent.
    ///
    /// # Errors
    ///
    /// Return an error if the configuration is invalid, providing a
    /// descriptive message about what failed.
    fn validate(&self) -> Result<()> {
        Ok(())
    }
}
