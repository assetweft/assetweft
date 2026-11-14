//! Responsibility: M2 immutable BYTE candidates and fixture staging, initialization and controlled byte validation.
//! Dependencies: M1 typed versions/CAS, Catalog initialization adapter, Production values, migration 005.
//! Callers: Application, single-database acceptance adapter and candidate integration tests.
//! Invariants: immutable descriptors, exact role sets, measured managed bytes; no
//! unauthorized pointer mutation, semantic fallback or caller-provided PASS; GLB structure only.
use crate::production::domain::report_passes;
use crate::{
    LockMode,
    library::{self, ComponentRole, LibraryError},
    storage,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Proposal {
    pub id: String,
    pub base_version: String,
    pub candidate_version: String,
    pub expected_revision: i64,
    pub editable: Vec<ComponentRole>,
    pub lock_mode: LockMode,
    pub contract: Option<FixtureContract>,
}

pub use crate::catalog::domain::{Current, FixtureContract};
/// Compatibility paths for Production and Catalog owned values.
pub use crate::production::domain::{
    Acceptance, AcceptanceReport, Candidate, ComponentEvidence, ComponentSnapshot,
};

fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && id != "."
        && id != ".."
}
fn invalid(s: &'static str) -> LibraryError {
    LibraryError::InvalidInput(s)
}
fn snapshot(
    conn: &mut Connection,
    root: &Path,
    owner: &str,
) -> Result<Vec<ComponentSnapshot>, LibraryError> {
    let refs = library::component_references(conn, owner)?;
    if refs.is_empty() || refs.len() > 7 {
        return Err(invalid("component count"));
    }
    let mut result: Vec<ComponentSnapshot> = Vec::new();
    for r in refs {
        if result.iter().any(|s| s.role == r.role.as_str()) {
            return Err(invalid("duplicate role"));
        }
        if !id_ok(&r.component_version_id) {
            return Err(invalid("component identity"));
        }
        let metadata = library::asset_version(conn, &r.component_version_id)?
            .ok_or_else(|| invalid("component metadata"))?;
        if metadata.source != "TEST_FIXTURE" {
            return Err(invalid("only declared TEST_FIXTURE staging supported"));
        }
        let object = measured(conn, root, &r.component_version_id)?;
        result.push(ComponentSnapshot {
            role: r.role.as_str().into(),
            version: r.component_version_id,
            measured_sha256: object.content_hash,
        });
    }
    Ok(result)
}

pub(crate) fn propose(
    conn: &mut Connection,
    root: &Path,
    p: &Proposal,
) -> Result<Candidate, LibraryError> {
    if ![&p.id, &p.base_version, &p.candidate_version]
        .iter()
        .all(|v| id_ok(v))
        || p.base_version == p.candidate_version
    {
        return Err(invalid("candidate identity"));
    }
    if p.expected_revision < 0 || p.expected_revision == i64::MAX {
        return Err(invalid("revision overflow"));
    }
    if p.lock_mode != LockMode::Byte {
        return Err(invalid("SEMANTIC NOT_RUN"));
    }
    if p.editable.is_empty() || p.editable.len() > 7 {
        return Err(invalid("editable roles"));
    }
    if let Some(contract) = &p.contract {
        check_contract(contract)?;
    }
    let mut editable = Vec::new();
    for role in &p.editable {
        let role = role.as_str().to_owned();
        if editable.contains(&role) {
            return Err(invalid("duplicate editable role"));
        }
        editable.push(role);
    }
    // Preserve exact descriptor order for idempotency, rather than normalizing changed reuse.
    let base_owner =
        library::asset_version(conn, &p.base_version)?.ok_or_else(|| invalid("base version"))?;
    let proposed_owner = library::asset_version(conn, &p.candidate_version)?
        .ok_or_else(|| invalid("candidate version"))?;
    if base_owner.source != "TEST_FIXTURE" || proposed_owner.source != "TEST_FIXTURE" {
        return Err(invalid("only declared TEST_FIXTURE staging supported"));
    }
    if base_owner.asset_id != proposed_owner.asset_id {
        return Err(invalid("candidate asset"));
    }
    let base = snapshot(conn, root, &p.base_version)?;
    let proposed = snapshot(conn, root, &p.candidate_version)?;
    if base.len() != proposed.len() || editable.iter().any(|r| !base.iter().any(|s| &s.role == r)) {
        return Err(invalid("role set"));
    }
    for before in &base {
        let after = proposed
            .iter()
            .find(|s| s.role == before.role)
            .ok_or_else(|| invalid("missing role"))?;
        if !editable.contains(&before.role) && before != after {
            return Err(invalid("locked component changed"));
        }
    }
    let candidate = Candidate {
        id: p.id.clone(),
        base_version: p.base_version.clone(),
        candidate_version: p.candidate_version.clone(),
        expected_revision: p.expected_revision,
        editable,
        base,
        proposed,
        source: "TEST_FIXTURE".into(),
        scope: "MANAGED_COMPONENT_BYTE_STAGING_ONLY".into(),
        contract: p.contract.clone(),
        not_run: vec![
            "FORMAT_AND_ROLE_VALIDATION".into(),
            "QUALITY".into(),
            "SEMANTIC".into(),
            "AUTHORIZATION_AND_LICENSE".into(),
            "CURRENT_REVISION_ACCEPTANCE".into(),
        ],
    };
    let encoded = serde_json::to_string(&candidate).map_err(|_| invalid("descriptor encoding"))?;
    // Existing versions retain their objects. Lease before SQLite prevents reference/GC races.
    let _guard = guard(root)?;
    // Application's reference insertion uses this same lease. Recheck after measurement.
    for (owner, measured) in [
        (&p.base_version, &candidate.base),
        (&p.candidate_version, &candidate.proposed),
    ] {
        let refs = library::component_references(conn, owner)?;
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
            [&p.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(old) = old {
        if old != encoded {
            return Err(invalid("changed candidate id reuse"));
        }
    } else {
        tx.execute("INSERT INTO aw_candidates(id,base_version,candidate_version,descriptor) VALUES(?1,?2,?3,?4)",params![p.id,p.base_version,p.candidate_version,encoded])?;
    }
    tx.commit()?;
    Ok(candidate)
}

pub(crate) fn read(conn: &Connection, id: &str) -> Result<Option<Candidate>, LibraryError> {
    if !id_ok(id) {
        return Err(invalid("candidate id"));
    }
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

fn check_contract(c: &FixtureContract) -> Result<(), LibraryError> {
    if c.contract_id != "TEST_FIXTURE_GLB_STRUCTURE_V1"
        || c.max_component_bytes == 0
        || c.max_component_bytes > 16 * 1024 * 1024
        || c.required_quality.len() > 32
        || c.required_quality.iter().any(|s| !id_ok(s))
    {
        return Err(invalid("fixture contract limits"));
    }
    Ok(())
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
    let metadata = library::asset_version(conn, id)?.ok_or_else(|| invalid("managed metadata"))?;
    library::validate(&metadata)?;
    let path = root.join("objects").join(&metadata.content_hash);
    let info = std::fs::symlink_metadata(path).map_err(|e| LibraryError::Io(e.to_string()))?;
    if !info.file_type().is_file() || info.len() > 16 * 1024 * 1024 {
        return Err(invalid("bounded managed regular file"));
    }
    wait_storage(|| match storage::managed(conn, root, id) {
        Err(storage::StorageError::Busy) => {
            Err(LibraryError::Busy("storage lease deadline".into()))
        }
        Err(storage::StorageError::Library(e)) => Err(e),
        Err(e) => Err(LibraryError::Io(e.to_string())),
        Ok(value) => value.ok_or_else(|| invalid("unmanaged component")),
    })
}
pub(crate) fn check_refs(
    conn: &mut Connection,
    owner: &str,
    measured: &[ComponentSnapshot],
) -> Result<(), LibraryError> {
    let refs = library::component_references(conn, owner)?;
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
    use crate::validation::{
        self, RoleRequirement, SourceClass, SourceDeclaration, ValidationLimits,
    };
    check_contract(contract)?;
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
        let metadata = library::asset_version(conn, &s.version)?
            .ok_or_else(|| invalid("component version"))?;
        if metadata.source != "TEST_FIXTURE" || metadata.source_format != "GLB" {
            report
                .not_run
                .push(format!("UNSUPPORTED_SOURCE_OR_FORMAT:{}", s.role));
            continue;
        }
        // CAS verification supplies a safe hash/path; bounded validator re-reads and hashes
        // the actual file. No SQL transaction spans parsing, no external URI is resolved.
        let checked = validation::validate_file(
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
/// Bootstrap only an absent fixture current; never reset an existing head/revision.
pub(crate) fn initialize_current(
    conn: &mut Connection,
    root: &Path,
    version: &str,
    contract: &FixtureContract,
) -> Result<Current, LibraryError> {
    if !id_ok(version) {
        return Err(invalid("version identity"));
    }
    check_contract(contract)?;
    if !contract.required_quality.is_empty() {
        return Err(invalid("required quality NOT_RUN"));
    }
    let owner = library::asset_version(conn, version)?.ok_or_else(|| invalid("base version"))?;
    if owner.source != "TEST_FIXTURE" || !id_ok(&owner.asset_id) {
        return Err(invalid("fixture base source/asset"));
    }
    let snapshots = snapshot(conn, root, version)?;
    let report = validate_components(conn, root, &snapshots, contract)?;
    if !report_passes(&report, snapshots.len()) {
        return Err(invalid("base GLB structure/role validation"));
    }
    let _guard = guard(root)?;
    check_refs(conn, version, &snapshots)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = crate::catalog::sqlite_store::initialize(&tx, &owner.asset_id, version, contract)?;
    tx.commit()?;
    Ok(result)
}
pub(crate) fn attempt_record(
    conn: &Connection,
    id: &str,
) -> Result<Option<Acceptance>, LibraryError> {
    if !id_ok(id) {
        return Err(invalid("candidate id"));
    }
    library::require_current(conn)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT receipt FROM aw_candidate_attempts WHERE candidate_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| serde_json::from_str(&s).map_err(|_| invalid("stored acceptance receipt")))
        .transpose()
}
