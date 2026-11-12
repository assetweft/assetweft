//! Responsibility: shared application entry and SQLite connection lifecycle.
//! Dependencies: the existing SQLite configuration and bounded file validation modules.
//! Callers: future CLI and desktop adapters; integration tests exercise the public API.
//! Invariants: no SQL or transaction escapes this module or the library repository;
//! validation is bounded, and no transaction is held while validation runs.

use crate::library::{self, AssetVersion, ComponentReference, LibraryError};
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
        library::check_open(&connection).map_err(LibraryError::into_open_error)?;
        Ok(Self { connection, config })
    }

    /// Configuration read back when this connection was opened.
    #[must_use]
    pub const fn connection_config(&self) -> &ConnectionConfig {
        &self.config
    }

    /// Initializes or checks the local product schema. This never performs a downgrade.
    ///
    /// # Errors
    /// Returns a migration, compatibility, or SQLite error without committing partial DDL.
    pub fn initialize_library(&mut self) -> Result<(), LibraryError> {
        library::initialize(&mut self.connection)
    }

    /// Records an asset identity and one metadata-only version in a short transaction.
    ///
    /// # Errors
    /// Rejects invalid fields, unsupported schema, duplicates and database errors.
    pub fn add_asset_version(&mut self, version: &AssetVersion) -> Result<(), LibraryError> {
        library::add_asset_version(&mut self.connection, version)
    }

    /// Reads a version by stable identity.
    ///
    /// # Errors
    /// Returns schema, unknown state, or database errors.
    pub fn asset_version(&mut self, id: &str) -> Result<Option<AssetVersion>, LibraryError> {
        library::asset_version(&mut self.connection, id)
    }

    /// Associates a component role with an existing version.
    ///
    /// # Errors
    /// Returns invalid input, foreign key, duplicate, schema or database errors.
    pub fn add_component_reference(
        &mut self,
        reference: &ComponentReference,
    ) -> Result<(), LibraryError> {
        library::add_component_reference(&mut self.connection, reference)
    }

    /// Reads explicitly versioned component references for an asset version.
    ///
    /// # Errors
    /// Returns schema, unknown role, or database errors.
    pub fn component_references(
        &mut self,
        version_id: &str,
    ) -> Result<Vec<ComponentReference>, LibraryError> {
        library::component_references(&mut self.connection, version_id)
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
