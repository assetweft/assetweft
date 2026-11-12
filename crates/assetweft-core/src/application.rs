//! Responsibility: shared application entry and SQLite connection lifecycle.
//! Dependencies: the existing SQLite configuration and bounded file validation modules.
//! Callers: future CLI and desktop adapters; integration tests exercise the public API.
//! Invariants: no SQL or transaction escapes this module or the library repository;
//! validation is bounded, and no transaction is held while validation runs.
//! Hash-reference insertion takes the storage lease before SQLite, like object collection.

use crate::library::{self, AssetVersion, ComponentReference, LibraryError};
use crate::sqlite::{self, ConnectionConfig, ProtoError};
use crate::validation::{
    self, RoleRequirement, SourceDeclaration, ValidationError, ValidationLimits, ValidationReport,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// One independently configured local application connection. Dropping it closes it;
/// explicit `close` reports SQLite close errors. No network or task executor exists yet.
pub struct Application {
    connection: Connection,
    library_root: PathBuf,
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
        let library_root = path
            .canonicalize()
            .map_err(|e| ProtoError::Io(e.to_string()))?
            .parent()
            .ok_or_else(|| ProtoError::Other("database parent required".into()))?
            .to_path_buf();
        Ok(Self {
            connection,
            config,
            library_root,
        })
    }

    /// Commits exact bytes before registering an immutable managed version.
    /// Input is bounded by expected length and max; an IO interruption cancels the commit.
    /// # Errors
    /// Reports input, IO, store-lock or database failure; published orphans can remain.
    pub fn commit_object(
        &mut self,
        version: &AssetVersion,
        reader: &mut impl std::io::Read,
        expected_len: u64,
        max_bytes: u64,
    ) -> Result<crate::storage::StoredObject, crate::storage::StorageError> {
        crate::storage::commit(
            &mut self.connection,
            &self.library_root,
            version,
            reader,
            expected_len,
            max_bytes,
        )
    }
    /// Resolves and rechecks managed bytes; metadata-only versions return None.
    /// # Errors
    /// Reports a missing/corrupt object, lock or database error.
    pub fn managed_object(
        &mut self,
        id: &str,
    ) -> Result<Option<crate::storage::StoredObject>, crate::storage::StorageError> {
        crate::storage::managed(&mut self.connection, &self.library_root, id)
    }
    /// Sets real object retention constraints; clearing them requires caller evidence.
    /// # Errors
    /// Rejects invalid/unknown hash, lock or database failure.
    pub fn protect_object(
        &mut self,
        hash: &str,
        backup_pin: bool,
        unsynced_unique: bool,
    ) -> Result<(), crate::storage::StorageError> {
        crate::storage::protect(
            &mut self.connection,
            &self.library_root,
            hash,
            backup_pin,
            unsynced_unique,
        )
    }
    /// Explicitly collects one unreferenced, unpinned, non-unique object; no sweep.
    /// # Errors
    /// Rejects unsafe paths/content, lock or database failure.
    pub fn collect_object(
        &mut self,
        hash: &str,
    ) -> Result<crate::storage::Collection, crate::storage::StorageError> {
        crate::storage::collect(&mut self.connection, &self.library_root, hash)
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

    /// Validates and persists a bounded local workflow before any step runs.
    /// # Errors
    /// Returns invalid dependencies, schema, or database errors.
    pub fn create_workflow(
        &mut self,
        run: &str,
        steps: &[crate::workflow::Step],
    ) -> Result<(), LibraryError> {
        crate::workflow::create(&mut self.connection, run, steps)
    }
    /// Atomically claims a ready step. A running step blocks further claims until reconciled.
    /// # Errors
    /// Returns schema, unknown run, or database errors.
    pub fn claim_workflow_step(&mut self, run: &str) -> Result<Option<String>, LibraryError> {
        crate::workflow::claim(&mut self.connection, run)
    }
    /// Explicitly records the actual outcome of a claimed step, including after restart.
    /// # Errors
    /// Returns invalid state, oversized result, schema, or database errors.
    pub fn finish_workflow_step(
        &mut self,
        run: &str,
        id: &str,
        outcome: &crate::workflow::Outcome,
    ) -> Result<(), LibraryError> {
        crate::workflow::finish(&mut self.connection, run, id, outcome)
    }
    /// Claims and invokes at most one caller-provided local driver outside any transaction.
    /// # Errors
    /// Returns claim or recording errors; an unresolved claim remains running.
    pub fn run_workflow_step(
        &mut self,
        run: &str,
        driver: impl FnOnce(&str) -> crate::workflow::Outcome,
    ) -> Result<bool, LibraryError> {
        let Some(id) = self.claim_workflow_step(run)? else {
            return Ok(false);
        };
        let outcome = driver(&id);
        self.finish_workflow_step(run, &id, &outcome)?;
        Ok(true)
    }
    /// Prevents future claims; does not stop a running callback or assert remote billing stopped.
    /// # Errors
    /// Returns unknown run, schema, or database errors.
    pub fn cancel_workflow(&mut self, run: &str) -> Result<(), LibraryError> {
        crate::workflow::cancel(&mut self.connection, run)
    }
    /// Reads whether cancellation was requested, without asserting remote work stopped.
    /// # Errors
    /// Returns unknown run, invalid state, schema, or database errors.
    pub fn workflow_cancelled(&self, run: &str) -> Result<bool, LibraryError> {
        crate::workflow::is_cancelled(&self.connection, run)
    }
    /// Reads persisted step states and exact outcomes.
    /// # Errors
    /// Returns invalid persisted state, unknown run, schema, or database errors.
    pub fn workflow_steps(
        &self,
        run: &str,
    ) -> Result<Vec<crate::workflow::StepRecord>, LibraryError> {
        crate::workflow::read(&self.connection, run)
    }

    /// Records an asset identity and one metadata-only version in a short transaction.
    ///
    /// # Errors
    /// Rejects invalid fields, unsupported schema, duplicates, storage-lease Busy/IO and database errors.
    pub fn add_asset_version(&mut self, version: &AssetVersion) -> Result<(), LibraryError> {
        library::validate(version)?;
        let _guard = crate::storage::reference_guard(&self.library_root)?;
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
        let _guard = crate::storage::reference_guard(&self.library_root)?;
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

// M2 typed policy entry; callbacks below are explicitly local TEST_FIXTURE only.
impl Application {
    /// Records immutable caller-declared capacity; not authenticated consent.
    pub fn approve_budget(&mut self, a: &crate::budget::Allowance) -> Result<(), LibraryError> {
        crate::budget::approve(&mut self.connection, a)
    }
    pub fn budget_balance(&self, id: &str) -> Result<crate::budget::Balance, LibraryError> {
        crate::budget::balance(&self.connection, id)
    }
    pub fn reserve_call(
        &mut self,
        r: &crate::budget::Request,
    ) -> Result<crate::budget::Intent, LibraryError> {
        crate::budget::reserve(&mut self.connection, r)
    }
    pub fn call_intent(&self, id: &str) -> Result<Option<crate::budget::Intent>, LibraryError> {
        crate::budget::intent(&self.connection, id)
    }
    /// One local counted callback, outside transactions. Panic leaves ATTEMPTING (uncertain).
    /// A returned callback also remains UNKNOWN until a separate explicit query receipt.
    /// No real provider execution or provider-side idempotency is implemented.
    pub fn submit_test_fixture(
        &mut self,
        id: &str,
        callback: impl FnOnce(),
    ) -> Result<bool, LibraryError> {
        if !crate::budget::claim(&mut self.connection, id)? {
            return Ok(false);
        }
        callback();
        crate::budget::unknown(&mut self.connection, id)?;
        Ok(true)
    }
    /// Cancellation releases a never-submitted preparing call, but retains uncertain reservations.
    pub fn cancel_call(&mut self, id: &str) -> Result<(), LibraryError> {
        crate::budget::cancel(&mut self.connection, id)
    }
    pub fn reconcile_call(
        &mut self,
        id: &str,
        r: &crate::budget::Receipt,
    ) -> Result<(), LibraryError> {
        crate::budget::reconcile(&mut self.connection, id, r)
    }
    pub fn call_receipts(&self, id: &str) -> Result<Vec<crate::budget::Receipt>, LibraryError> {
        crate::budget::receipts(&self.connection, id)
    }
}

impl Application {
    /// Stages measured managed BYTE components. Never authorizes publication.
    /// # Errors
    /// Rejects semantic locks, changed locked refs, corrupt bytes and invalid descriptors.
    pub fn propose_candidate(
        &mut self,
        proposal: &crate::candidate::Proposal,
    ) -> Result<crate::candidate::Candidate, LibraryError> {
        crate::candidate::propose(&mut self.connection, &self.library_root, proposal)
    }
    /// Reads the immutable staged descriptor and its explicit validation scope.
    /// # Errors
    /// Returns schema, database or malformed identity errors.
    pub fn candidate(&self, id: &str) -> Result<Option<crate::candidate::Candidate>, LibraryError> {
        crate::candidate::read(&self.connection, id)
    }
    /// Runs the predeclared fixture structure contract and conditionally advances current.
    /// Returns an immutable historic receipt on replay, not a fresh validation claim.
    /// # Errors
    /// Returns missing/corrupt bytes, schema, lease or transaction errors.
    pub fn attempt_accept_candidate(
        &mut self,
        id: &str,
    ) -> Result<crate::candidate::Acceptance, LibraryError> {
        crate::candidate::attempt(&mut self.connection, &self.library_root, id)
    }
    /// Reads the durable terminal receipt, including conflicts and its candidate-bound report.
    /// # Errors
    /// Returns schema, identity or stored encoding errors.
    pub fn candidate_attempt(
        &self,
        id: &str,
    ) -> Result<Option<crate::candidate::Acceptance>, LibraryError> {
        crate::candidate::attempt_record(&self.connection, id)
    }
    /// Initializes an absent fixture-only current head at revision zero after GLB checks.
    /// # Errors
    /// Rejects required unimplemented quality, unsupported roles, existing head and IO/DB errors.
    pub fn initialize_candidate_current(
        &mut self,
        version: &str,
        contract: &crate::candidate::FixtureContract,
    ) -> Result<crate::candidate::Current, LibraryError> {
        crate::candidate::initialize_current(
            &mut self.connection,
            &self.library_root,
            version,
            contract,
        )
    }
    /// Reads the local fixture current version, revision, and immutable declared contract.
    /// # Errors
    /// Returns schema, identity or database errors.
    pub fn candidate_current(
        &self,
        asset: &str,
    ) -> Result<Option<crate::candidate::Current>, LibraryError> {
        crate::candidate::current(&self.connection, asset)
    }
}
