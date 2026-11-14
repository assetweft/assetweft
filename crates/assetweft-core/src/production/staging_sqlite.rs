//! Responsibility: concrete SQLite candidate freeze/read and bounded managed-byte verification.
//! Dependencies: Catalog concrete reads, CAS lease, rusqlite and Assurance checks; callers: Application, acceptance and initialization adapters.
//! Invariants: encoding before lease, reference recheck under lease before IMMEDIATE, unchanged SQL/parameters, no replay.
use super::{
    domain::{AcceptanceReport, Candidate, ComponentEvidence, ComponentSnapshot},
    staging_application::FrozenProposal,
    staging_domain::check_contract,
    staging_ports::{Owners, Staging, StagingError},
};
use crate::catalog::domain::FixtureContract;
use crate::{
    library::{self, LibraryError},
    storage,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::Path;
fn invalid(s: &'static str) -> LibraryError {
    LibraryError::InvalidInput(s)
}
pub(crate) struct SqliteStaging<'a> {
    pub conn: &'a mut Connection,
    pub root: &'a Path,
}
impl Staging for SqliteStaging<'_> {
    fn owners(&mut self, base: &str, proposed: &str) -> Result<Owners, StagingError> {
        let base = crate::catalog::sqlite_metadata::asset_version(self.conn, base)?
            .ok_or(StagingError::InvalidInput("base version"))?;
        let proposed = crate::catalog::sqlite_metadata::asset_version(self.conn, proposed)?
            .ok_or(StagingError::InvalidInput("candidate version"))?;
        Ok(Owners {
            base_source: base.source,
            proposed_source: proposed.source,
            base_asset: base.asset_id,
            proposed_asset: proposed.asset_id,
        })
    }
    fn snapshot(&mut self, owner: &str) -> Result<Vec<ComponentSnapshot>, StagingError> {
        snapshot(self.conn, self.root, owner).map_err(Into::into)
    }
    fn freeze(&mut self, proposal: FrozenProposal) -> Result<Candidate, StagingError> {
        freeze(self.conn, self.root, proposal.into_candidate()).map_err(Into::into)
    }
}
fn freeze(
    conn: &mut Connection,
    root: &Path,
    candidate: Candidate,
) -> Result<Candidate, LibraryError> {
    let encoded = serde_json::to_string(&candidate).map_err(|_| invalid("descriptor encoding"))?;
    // Existing versions retain their objects. Lease before SQLite prevents reference/GC races.
    let _guard = guard(root)?;
    // Application's reference insertion uses this same lease. Recheck after measurement.
    for (owner, measured) in [
        (&candidate.base_version, &candidate.base),
        (&candidate.candidate_version, &candidate.proposed),
    ] {
        let refs = crate::catalog::sqlite_metadata::component_references(conn, owner)?;
        if refs.len() != measured.len()
            || refs
                .iter()
                .zip(measured)
                .any(|(r, s)| r.role.as_str() != s.role || r.component_version_id != s.version)
        {
            return Err(invalid("references changed during measurement"));
        }
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    library::require_current(&tx)?;
    let old: Option<String> = tx
        .query_row(
            "SELECT descriptor FROM aw_candidates WHERE id=?1",
            [&candidate.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(old) = old {
        if old != encoded {
            return Err(invalid("changed candidate id reuse"));
        }
    } else {
        tx.execute("INSERT INTO aw_candidates(id,base_version,candidate_version,descriptor) VALUES(?1,?2,?3,?4)",params![candidate.id,candidate.base_version,candidate.candidate_version,encoded])?;
    }
    tx.commit()?;
    Ok(candidate)
}
pub(crate) fn read(conn: &Connection, id: &str) -> Result<Option<Candidate>, LibraryError> {
    library::require_current(conn)?;
    let value: Option<String> = conn
        .query_row(
            "SELECT descriptor FROM aw_candidates WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    value
        .map(|s| serde_json::from_str(&s).map_err(|_| invalid("stored descriptor")))
        .transpose()
}
pub(crate) fn snapshot(
    conn: &mut Connection,
    root: &Path,
    owner: &str,
) -> Result<Vec<ComponentSnapshot>, LibraryError> {
    let refs = crate::catalog::sqlite_metadata::component_references(conn, owner)?;
    super::staging_domain::component_count(refs.len()).map_err(LibraryError::from)?;
    let mut result: Vec<ComponentSnapshot> = Vec::new();
    for r in refs {
        super::staging_domain::component_identity(
            &result,
            r.role.as_str(),
            &r.component_version_id,
        )
        .map_err(LibraryError::from)?;
        let metadata =
            crate::catalog::sqlite_metadata::asset_version(conn, &r.component_version_id)?
                .ok_or_else(|| invalid("component metadata"))?;
        super::staging_domain::component_source(&metadata.source).map_err(LibraryError::from)?;
        let object = measured(conn, root, &r.component_version_id)?;
        result.push(ComponentSnapshot {
            role: r.role.as_str().into(),
            version: r.component_version_id,
            measured_sha256: object.content_hash,
        });
    }
    Ok(result)
}

// Retry only lease acquisition/read-only verification before any write transaction.
// A short fixed attempt count starved a concurrent App under fsync contention.
// A monotonic two-second deadline accommodates a brief holder and still returns Busy
// for an abandoned lease. Never replay a conditional UPDATE or its receipt transaction.
fn wait_storage<T>(
    mut operation: impl FnMut() -> Result<T, LibraryError>,
) -> Result<T, LibraryError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match operation() {
            Err(LibraryError::Busy(_)) if std::time::Instant::now() < deadline => {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                std::thread::sleep(crate::sqlite::BUSY_BACKOFF.min(remaining));
            }
            result => return result,
        }
    }
}
pub(crate) fn guard(root: &Path) -> Result<storage::Guard, LibraryError> {
    wait_storage(|| storage::reference_guard(root))
}
pub(crate) fn measured(
    conn: &mut Connection,
    root: &Path,
    id: &str,
) -> Result<storage::StoredObject, LibraryError> {
    let metadata = crate::catalog::sqlite_metadata::asset_version(conn, id)?
        .ok_or_else(|| invalid("managed metadata"))?;
    crate::catalog::domain::validate_version(&metadata).map_err(LibraryError::from)?;
    let path = root.join("objects").join(&metadata.content_hash);
    let info = std::fs::symlink_metadata(path).map_err(|e| LibraryError::Io(e.to_string()))?;
    if !info.file_type().is_file() || info.len() > 16 * 1024 * 1024 {
        return Err(invalid("bounded managed regular file"));
    }
    wait_storage(|| {
        match crate::catalog::objects_application::managed(
            &mut crate::catalog::objects_sqlite::SqliteObjects::new(conn, root, std::io::empty()),
            id,
        ) {
            Err(storage::StorageError::Busy) => {
                Err(LibraryError::Busy("storage lease deadline".into()))
            }
            Err(storage::StorageError::Library(e)) => Err(e),
            Err(e) => Err(LibraryError::Io(e.to_string())),
            Ok(value) => value.ok_or_else(|| invalid("unmanaged component")),
        }
    })
}
pub(crate) fn check_refs(
    conn: &mut Connection,
    owner: &str,
    measured: &[ComponentSnapshot],
) -> Result<(), LibraryError> {
    let refs = crate::catalog::sqlite_metadata::component_references(conn, owner)?;
    if refs.len() != measured.len()
        || refs
            .iter()
            .zip(measured)
            .any(|(r, s)| r.role.as_str() != s.role || r.component_version_id != s.version)
    {
        return Err(invalid("candidate reference drift"));
    }
    Ok(())
}
pub(crate) fn validate_components(
    conn: &mut Connection,
    root: &Path,
    snapshots: &[ComponentSnapshot],
    contract: &FixtureContract,
) -> Result<AcceptanceReport, LibraryError> {
    use crate::assurance::{
        RoleRequirement, SourceClass, SourceDeclaration, ValidationLimits, checks,
    };
    check_contract(contract).map_err(LibraryError::from)?;
    let mut report = AcceptanceReport {
        scope: "TEST_FIXTURE_GLB_STRUCTURE_V1".into(),
        source: "TEST_FIXTURE_DECLARATION_UNVERIFIED".into(),
        components: Vec::new(),
        not_run: vec![
            "QUALITY".into(),
            "SEMANTIC".into(),
            "AUTHORIZATION_AND_LICENSE".into(),
            "PRODUCTION".into(),
        ],
    };
    for s in snapshots {
        // The mature checker has no standalone material/audio/VFX role contract.
        let role = match s.role.as_str() {
            "mesh" => RoleRequirement::StaticMesh,
            "rig" => RoleRequirement::RiggedCharacter,
            "motion" => RoleRequirement::AnimatedCharacter,
            _ => {
                report
                    .not_run
                    .push(format!("UNSUPPORTED_COMPONENT_ROLE:{}", s.role));
                continue;
            }
        };
        let object = measured(conn, root, &s.version)?;
        if object.content_hash != s.measured_sha256 {
            return Err(invalid("candidate byte drift"));
        }
        if object.byte_length > contract.max_component_bytes {
            return Err(invalid("component byte limit"));
        }
        let metadata = crate::catalog::sqlite_metadata::asset_version(conn, &s.version)?
            .ok_or_else(|| invalid("component version"))?;
        if metadata.source != "TEST_FIXTURE" || metadata.source_format != "GLB" {
            report
                .not_run
                .push(format!("UNSUPPORTED_SOURCE_OR_FORMAT:{}", s.role));
            continue;
        }
        // CAS verification supplies a safe hash/path; bounded validator re-reads and hashes
        // the actual file. No SQL transaction spans parsing, no external URI is resolved.
        let checked = checks::validate_file(
            &root.join("objects").join(&object.content_hash),
            role,
            SourceDeclaration::declared(SourceClass::TestFixture),
            ValidationLimits {
                max_bytes: contract.max_component_bytes,
            },
        )
        .map_err(|e| LibraryError::Io(e.to_string()))?;
        if checked.file_sha256 != s.measured_sha256 {
            return Err(invalid("validation byte drift"));
        }
        report.components.push(ComponentEvidence {
            role: s.role.clone(),
            version_id: s.version.clone(),
            sha256: checked.file_sha256,
            checker_version: checked.checker_version.into(),
            requirement: role.as_str().into(),
            format_status: checked.format_status.as_str().into(),
            role_status: checked.role.status.as_str().into(),
            not_run: checked.not_run.iter().map(|s| (*s).into()).collect(),
        });
    }
    Ok(report)
}
pub(crate) struct SqliteCandidateRead<'a> {
    pub conn: &'a Connection,
}
impl super::staging_ports::CandidateRead for SqliteCandidateRead<'_> {
    fn candidate(&mut self, id: &str) -> Result<Option<Candidate>, StagingError> {
        read(self.conn, id).map_err(Into::into)
    }
}
