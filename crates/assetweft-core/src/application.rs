//! Responsibility: shared application entry and SQLite connection lifecycle.
//! Dependencies: the existing SQLite configuration and bounded file validation modules.
//! Callers: future CLI and desktop adapters; integration tests exercise the public API.
//! Invariants: no SQL or transaction escapes this module; no product schema is created;
//! validation is bounded, and no transaction is held while validation runs.

use crate::sqlite::{self, ConnectionConfig, ProtoError};
use crate::validation::{
    self, RoleRequirement, SourceDeclaration, ValidationError, ValidationLimits, ValidationReport,
};
use rusqlite::Connection;
use std::path::Path;

/// One independently configured local application connection. Dropping it closes it;
/// explicit `close` reports SQLite close errors. No network or task executor exists yet.
pub struct Application {
    connection: Connection,
    config: ConnectionConfig,
}

impl Application {
    /// Opens a local file database, creating it if necessary. Parent directories must exist.
    ///
    /// # Errors
    /// Rejects SQLite memory/URI paths and returns configuration or filesystem errors.
    pub fn open(path: &Path) -> Result<Self, ProtoError> {
        let raw = path.as_os_str();
        if raw.is_empty() || raw == ":memory:" || raw.to_string_lossy().starts_with("file:") {
            return Err(ProtoError::Other(
                "a local SQLite file path is required".to_owned(),
            ));
        }
        let (connection, config) = sqlite::open_and_configure(path)?;
        Ok(Self { connection, config })
    }

    /// Configuration read back when this connection was opened.
    #[must_use]
    pub const fn connection_config(&self) -> &ConnectionConfig {
        &self.config
    }

    /// Reads SQLite's internal schema cookie as a connection health check. This is
    /// not the application's future migration version.
    ///
    /// # Errors
    /// Returns the SQLite read error, including lock contention.
    pub fn schema_version(&self) -> Result<i64, rusqlite::Error> {
        self.connection
            .pragma_query_value(None, "schema_version", |row| row.get(0))
    }

    /// Runs existing bounded file validation without starting a database transaction.
    ///
    /// # Errors
    /// Propagates file access and size-limit errors from the validation module.
    pub fn validate_file(
        &self,
        path: &Path,
        requirement: RoleRequirement,
        source: SourceDeclaration,
        limits: ValidationLimits,
    ) -> Result<ValidationReport, ValidationError> {
        validation::validate_file(path, requirement, source, limits)
    }

    /// Closes the connection. On failure the returned error is from rusqlite; the
    /// connection is dropped and cannot be reused. No pending work is created here.
    ///
    /// # Errors
    /// Returns the SQLite error if an explicit close fails.
    pub fn close(self) -> Result<(), rusqlite::Error> {
        self.connection
            .close()
            .map_err(|(_connection, error)| error)
    }
}
