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
    /// Creates an exclusive new SQLite/CAS backup, retaining objects until completion.
    /// # Errors
    /// Reports lease, schema, integrity, cancellation, limits and filesystem failures.
    pub fn backup_library(
        &self,
        destination: &Path,
        limits: crate::backup::Limits,
        progress: &mut dyn FnMut(crate::backup::Progress) -> bool,
    ) -> Result<crate::backup::Manifest, crate::backup::BackupError> {
        crate::catalog::backup_application::create(
            &mut crate::catalog::backup_sqlite::BackupSession::new(
                &self.connection,
                &self.library_root,
                destination,
                limits,
                progress,
            ),
        )
    }
    /// Restores a supported snapshot into an exclusively new directory.
    /// # Errors
    /// Reports unsupported or corrupt packages, existing destinations and IO failures.
    pub fn restore_library(
        source: &Path,
        destination: &Path,
        limits: crate::backup::Limits,
        progress: &mut dyn FnMut(crate::backup::Progress) -> bool,
    ) -> Result<crate::backup::Manifest, crate::backup::BackupError> {
        let mut adapter = crate::catalog::backup_sqlite::RestoreSession::new(
            source,
            destination,
            limits,
            progress,
        )?;
        crate::catalog::backup_application::restore(&mut adapter, limits)
    }

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
        let mut adapter = crate::catalog::objects_sqlite::SqliteObjects::new(
            &mut self.connection,
            &self.library_root,
            reader,
        );
        crate::catalog::objects_application::commit(&mut adapter, version, expected_len, max_bytes)
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
        let mut adapter = crate::catalog::objects_sqlite::SqliteObjects::new(
            &mut self.connection,
            &self.library_root,
            std::io::empty(),
        );
        crate::catalog::objects_application::protect(
            &mut adapter,
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
        let mut adapter = crate::catalog::objects_sqlite::SqliteObjects::new(
            &mut self.connection,
            &self.library_root,
            std::io::empty(),
        );
        crate::catalog::objects_application::collect(&mut adapter, hash)
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
        crate::production::workflow_application::create(
            &mut crate::production::workflow_sqlite::SqliteWorkflowRuns(&mut self.connection),
            run,
            steps,
        )
    }
    /// Atomically claims a ready step. A running step blocks further claims until reconciled.
    /// # Errors
    /// Returns schema, unknown run, or database errors.
    pub fn claim_workflow_step(&mut self, run: &str) -> Result<Option<String>, LibraryError> {
        crate::production::workflow_application::claim(
            &mut crate::production::workflow_sqlite::SqliteWorkflowRuns(&mut self.connection),
            run,
        )
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
        crate::production::workflow_application::finish(
            &mut crate::production::workflow_sqlite::SqliteWorkflowRuns(&mut self.connection),
            run,
            id,
            outcome,
        )
    }
    /// Claims and invokes at most one caller-provided local driver outside any transaction.
    /// # Errors
    /// Returns claim or recording errors; an unresolved claim remains running.
    pub fn run_workflow_step(
        &mut self,
        run: &str,
        driver: impl FnOnce(&str) -> crate::workflow::Outcome,
    ) -> Result<bool, LibraryError> {
        crate::production::workflow_application::run(
            &mut crate::production::workflow_sqlite::SqliteWorkflowRuns(&mut self.connection),
            run,
            driver,
        )
    }
    /// Prevents future claims; does not stop a running callback or assert remote billing stopped.
    /// # Errors
    /// Returns unknown run, schema, or database errors.
    pub fn cancel_workflow(&mut self, run: &str) -> Result<(), LibraryError> {
        crate::production::workflow_application::cancel(
            &mut crate::production::workflow_sqlite::SqliteWorkflowRuns(&mut self.connection),
            run,
        )
    }
    /// Reads whether cancellation was requested, without asserting remote work stopped.
    /// # Errors
    /// Returns unknown run, invalid state, schema, or database errors.
    pub fn workflow_cancelled(&self, run: &str) -> Result<bool, LibraryError> {
        crate::production::workflow_application::is_cancelled(
            &crate::production::workflow_sqlite::SqliteWorkflowRuns(&self.connection),
            run,
        )
    }
    /// Reads persisted step states and exact outcomes.
    /// # Errors
    /// Returns invalid persisted state, unknown run, schema, or database errors.
    pub fn workflow_steps(
        &self,
        run: &str,
    ) -> Result<Vec<crate::workflow::StepRecord>, LibraryError> {
        crate::production::workflow_application::read(
            &crate::production::workflow_sqlite::SqliteWorkflowRuns(&self.connection),
            run,
        )
    }

    /// Records an asset identity and one metadata-only version in a short transaction.
    ///
    /// # Errors
    /// Rejects invalid fields, unsupported schema, duplicates, storage-lease Busy/IO and database errors.
    pub fn add_asset_version(&mut self, version: &AssetVersion) -> Result<(), LibraryError> {
        let mut metadata = crate::catalog::sqlite_metadata::SqliteMetadata::new(
            &mut self.connection,
            &self.library_root,
        );
        crate::catalog::application::register_version(&mut metadata, version).map_err(Into::into)
    }

    /// Reads a version by stable identity.
    ///
    /// # Errors
    /// Returns schema, unknown state, or database errors.
    pub fn asset_version(&mut self, id: &str) -> Result<Option<AssetVersion>, LibraryError> {
        let mut metadata = crate::catalog::sqlite_metadata::SqliteMetadata::new(
            &mut self.connection,
            &self.library_root,
        );
        crate::catalog::application::exact_version(&mut metadata, id).map_err(Into::into)
    }

    /// Associates a component role with an existing version.
    ///
    /// # Errors
    /// Returns invalid input, foreign key, duplicate, schema or database errors.
    pub fn add_component_reference(
        &mut self,
        reference: &ComponentReference,
    ) -> Result<(), LibraryError> {
        let mut metadata = crate::catalog::sqlite_metadata::SqliteMetadata::with_reference_lease(
            &mut self.connection,
            &self.library_root,
        )?;
        crate::catalog::application::register_reference(&mut metadata, reference)
            .map_err(Into::into)
    }

    /// Reads explicitly versioned component references for an asset version.
    ///
    /// # Errors
    /// Returns schema, unknown role, or database errors.
    pub fn component_references(
        &mut self,
        version_id: &str,
    ) -> Result<Vec<ComponentReference>, LibraryError> {
        let mut metadata = crate::catalog::sqlite_metadata::SqliteMetadata::new(
            &mut self.connection,
            &self.library_root,
        );
        crate::catalog::application::exact_references(&mut metadata, version_id).map_err(Into::into)
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
        crate::consumption::application::approve(
            &mut crate::consumption::sqlite::SqliteBudgetLedger(&mut self.connection),
            a,
        )
        .map_err(Into::into)
    }
    pub fn budget_balance(&self, id: &str) -> Result<crate::budget::Balance, LibraryError> {
        crate::consumption::application::balance(
            &mut crate::consumption::sqlite::BudgetQuery(&self.connection),
            id,
        )
        .map_err(Into::into)
    }
    pub fn reserve_call(
        &mut self,
        r: &crate::budget::Request,
    ) -> Result<crate::budget::Intent, LibraryError> {
        crate::consumption::application::reserve(
            &mut crate::consumption::sqlite::SqliteBudgetLedger(&mut self.connection),
            r,
        )
        .map_err(Into::into)
    }
    pub fn call_intent(&self, id: &str) -> Result<Option<crate::budget::Intent>, LibraryError> {
        crate::consumption::application::intent(
            &mut crate::consumption::sqlite::BudgetQuery(&self.connection),
            id,
        )
        .map_err(Into::into)
    }
    /// One local counted callback, outside transactions. Panic leaves ATTEMPTING (uncertain).
    /// A returned callback also remains UNKNOWN until a separate explicit query receipt.
    /// No real provider execution or provider-side idempotency is implemented.
    pub fn submit_test_fixture(
        &mut self,
        id: &str,
        callback: impl FnOnce(),
    ) -> Result<bool, LibraryError> {
        crate::consumption::application::submit_test_fixture(
            &mut crate::consumption::sqlite::SqliteBudgetLedger(&mut self.connection),
            id,
            callback,
        )
        .map_err(Into::into)
    }
    /// Cancellation releases a never-submitted preparing call, but retains uncertain reservations.
    pub fn cancel_call(&mut self, id: &str) -> Result<(), LibraryError> {
        crate::consumption::application::cancel(
            &mut crate::consumption::sqlite::SqliteBudgetLedger(&mut self.connection),
            id,
        )
        .map_err(Into::into)
    }
    pub fn reconcile_call(
        &mut self,
        id: &str,
        r: &crate::budget::Receipt,
    ) -> Result<(), LibraryError> {
        crate::consumption::application::reconcile(
            &mut crate::consumption::sqlite::SqliteBudgetLedger(&mut self.connection),
            id,
            r,
        )
        .map_err(Into::into)
    }
    pub fn call_receipts(&self, id: &str) -> Result<Vec<crate::budget::Receipt>, LibraryError> {
        crate::consumption::application::receipts(
            &mut crate::consumption::sqlite::BudgetQuery(&self.connection),
            id,
        )
        .map_err(Into::into)
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
        crate::production::application::attempt(
            &mut crate::sqlite::acceptance::SqliteAcceptance {
                conn: &mut self.connection,
                root: &self.library_root,
            },
            id,
        )
        .map_err(Into::into)
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
        crate::catalog::head_application::current(
            &mut crate::catalog::head_sqlite::SqliteHead(&self.connection),
            asset,
        )
        .map_err(Into::into)
    }
}

// M1 opinions remain separate from M2 candidate acceptance.
impl Application {
    /// Appends an exact-version opinion; duplicate feedback IDs fail without overwriting.
    /// # Errors
    /// Rejects invalid bounds, missing/mismatched versions, duplicates and database failures.
    pub fn append_feedback(
        &mut self,
        feedback: &crate::history::Feedback,
    ) -> Result<crate::history::FeedbackRecord, LibraryError> {
        crate::catalog::feedback::append(
            &mut crate::catalog::sqlite_feedback::SqliteFeedback(&mut self.connection),
            feedback,
        )
        .map_err(Into::into)
    }
    /// Returns bounded newest-first history; absent assets/versions yield an empty list.
    /// # Errors
    /// Rejects invalid bounds, schema errors and unknown stored decisions.
    pub fn feedback_history(
        &mut self,
        query: crate::history::HistoryQuery<'_>,
    ) -> Result<Vec<crate::history::FeedbackRecord>, LibraryError> {
        crate::catalog::feedback::history(
            &mut crate::catalog::sqlite_feedback::SqliteFeedback(&mut self.connection),
            query,
        )
        .map_err(Into::into)
    }
    /// Latest opinion for an asset or exact version, independent of candidate/current status.
    /// # Errors
    /// Returns invalid identity, schema or database errors.
    pub fn latest_feedback(
        &mut self,
        asset_id: &str,
        version_id: Option<&str>,
    ) -> Result<Option<crate::history::FeedbackRecord>, LibraryError> {
        crate::catalog::feedback::latest(
            &mut crate::catalog::sqlite_feedback::SqliteFeedback(&mut self.connection),
            asset_id,
            version_id,
        )
        .map_err(Into::into)
    }
}

impl Application {
    /// Typed LOCAL TEST_FIXTURE engineering only; not authenticated provider admission.
    /// # Errors
    /// Returns preflight failures; post-invocation recording errors retain the backend result.
    pub fn submit_executor_local_test_fixture<B: crate::executor::Backend>(
        &mut self,
        executor: &mut crate::executor::Executor<B>,
        budget: &crate::budget::Request,
        request: &crate::executor::SubmitRequest,
    ) -> Result<crate::executor_runtime::Submission, crate::executor_runtime::PreflightError> {
        crate::execution::runtime_application::submit_local_test_fixture(
            &mut crate::execution::consumption_adapter::ConsumptionBudgetOnce(&mut self.connection),
            executor,
            budget,
            request,
        )
    }
}
