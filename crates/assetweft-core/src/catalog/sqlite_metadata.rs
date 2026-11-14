//! Responsibility: Catalog SQLite immutable metadata and exact-reference persistence.
//! Dependencies: rusqlite, existing library migration/error guard and storage retention lease.
//! Callers: Application metadata use cases, Production staging/acceptance/initialization adapters and Catalog objects_sqlite.
//! Invariants: lease before IMMEDIATE; asset+version atomic; borrowed insertion never commits.
use super::{
    domain::{self, AssetVersion, ComponentReference, ComponentRole, MetadataError, Presence},
    ports::Metadata,
};
use crate::{
    library::{self, LibraryError},
    storage,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::Path;

pub(crate) struct SqliteMetadata<'a> {
    conn: &'a mut Connection,
    root: &'a Path,
    reference_lease: Option<storage::Guard>,
}
impl<'a> SqliteMetadata<'a> {
    pub(crate) fn new(conn: &'a mut Connection, root: &'a Path) -> Self {
        Self {
            conn,
            root,
            reference_lease: None,
        }
    }
    // Composition must call this before invoking reference validation, preserving Busy precedence.
    pub(crate) fn with_reference_lease(
        conn: &'a mut Connection,
        root: &'a Path,
    ) -> Result<Self, LibraryError> {
        let lease = storage::reference_guard(root)?;
        Ok(Self {
            conn,
            root,
            reference_lease: Some(lease),
        })
    }
}
impl Metadata for SqliteMetadata<'_> {
    fn register_version(&mut self, v: &AssetVersion) -> Result<(), MetadataError> {
        let _lease = storage::reference_guard(self.root).map_err(MetadataError::from)?;
        add_asset_version(self.conn, v).map_err(Into::into)
    }
    fn exact_version(&mut self, id: &str) -> Result<Option<AssetVersion>, MetadataError> {
        asset_version(self.conn, id).map_err(Into::into)
    }
    fn register_reference(&mut self, r: &ComponentReference) -> Result<(), MetadataError> {
        // Even a future direct port caller cannot perform a successful unguarded write.
        let _lease = if self.reference_lease.is_none() {
            Some(storage::reference_guard(self.root).map_err(MetadataError::from)?)
        } else {
            None
        };
        add_component_reference(self.conn, r).map_err(Into::into)
    }
    fn exact_references(&mut self, id: &str) -> Result<Vec<ComponentReference>, MetadataError> {
        component_references(self.conn, id).map_err(Into::into)
    }
}
impl From<LibraryError> for MetadataError {
    fn from(error: LibraryError) -> Self {
        match error {
            LibraryError::Busy(s) => Self::Busy(s),
            LibraryError::Constraint(s) => Self::Constraint(s),
            LibraryError::Io(s) => Self::Io(s),
            LibraryError::Database(s) => Self::Database(s),
            LibraryError::FutureSchema(v) => Self::FutureSchema(v),
            LibraryError::Uninitialized => Self::Uninitialized,
            LibraryError::InvalidInput(s) => Self::InvalidInput(s),
            LibraryError::UnknownValue(s) => Self::UnknownValue(s),
        }
    }
}
impl From<MetadataError> for LibraryError {
    fn from(error: MetadataError) -> Self {
        match error {
            MetadataError::Busy(s) => Self::Busy(s),
            MetadataError::Constraint(s) => Self::Constraint(s),
            MetadataError::Io(s) => Self::Io(s),
            MetadataError::Database(s) => Self::Database(s),
            MetadataError::FutureSchema(v) => Self::FutureSchema(v),
            MetadataError::Uninitialized => Self::Uninitialized,
            MetadataError::InvalidInput(s) => Self::InvalidInput(s),
            MetadataError::UnknownValue(s) => Self::UnknownValue(s),
        }
    }
}

fn add_asset_version(conn: &mut Connection, v: &AssetVersion) -> Result<(), LibraryError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    library::require_current(&tx)?;
    insert_version(&tx, v)?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn asset_version(
    conn: &mut Connection,
    id: &str,
) -> Result<Option<AssetVersion>, LibraryError> {
    domain::validate_version_id(id).map_err(LibraryError::from)?;
    let tx = conn.transaction()?;
    library::require_current(&tx)?;
    let result = tx.query_row("SELECT asset_id,version_id,content_hash,input_id,source,authorization,authorized,license_id,usage_scope,source_format,contract_version,presence FROM aw_asset_versions WHERE version_id=?1", [id], |r| {
        Ok((AssetVersion { asset_id:r.get(0)?, version_id:r.get(1)?, content_hash:r.get(2)?, input_id:r.get(3)?, source:r.get(4)?, authorization:r.get(5)?, authorized:r.get(6)?, license_id:r.get(7)?, usage_scope:r.get(8)?, source_format:r.get(9)?, contract_version:r.get(10)?, presence: Presence::Unknown }, r.get::<_, String>(11)?))
    }).optional()?;
    let result = result
        .map(|(mut v, state)| {
            if v.contract_version != "1.0.0" {
                return Err(LibraryError::UnknownValue(v.contract_version));
            }
            v.presence = Presence::parse(&state)?;
            Ok::<_, LibraryError>(v)
        })
        .transpose()?;
    tx.commit()?;
    Ok(result)
}
fn add_component_reference(
    conn: &mut Connection,
    r: &ComponentReference,
) -> Result<(), LibraryError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    library::require_current(&tx)?;
    tx.execute("INSERT INTO aw_component_references(owner_version_id,role,component_version_id) VALUES(?1,?2,?3)", params![r.owner_version_id, r.role.as_str(), r.component_version_id])?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn component_references(
    conn: &mut Connection,
    id: &str,
) -> Result<Vec<ComponentReference>, LibraryError> {
    domain::validate_version_id(id).map_err(LibraryError::from)?;
    let tx = conn.transaction()?;
    library::require_current(&tx)?;
    let rows = {
        let mut stmt = tx.prepare("SELECT owner_version_id,role,component_version_id FROM aw_component_references WHERE owner_version_id=?1 ORDER BY role,component_version_id")?;
        let mapped = stmt.query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut output = Vec::new();
        for row in mapped {
            let (owner_version_id, role, component_version_id) = row?;
            output.push(ComponentReference {
                owner_version_id,
                role: ComponentRole::parse(&role)?,
                component_version_id,
            });
        }
        output
    };
    tx.commit()?;
    Ok(rows)
}

// Shared insertion in the caller's checked short transaction.
pub(crate) fn insert_version(tx: &Connection, v: &AssetVersion) -> Result<(), LibraryError> {
    tx.execute(
        "INSERT INTO aw_assets(asset_id) VALUES (?1) ON CONFLICT(asset_id) DO NOTHING",
        [&v.asset_id],
    )?;
    tx.execute("INSERT INTO aw_asset_versions(version_id,asset_id,content_hash,input_id,source,authorization,authorized,license_id,usage_scope,source_format,contract_version,presence) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![v.version_id, v.asset_id, v.content_hash, v.input_id, v.source, v.authorization, v.authorized, v.license_id, v.usage_scope, v.source_format, v.contract_version, v.presence.as_str()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::Application;
    #[test]
    fn facade_preserves_distinct_malformed_input_and_busy_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = Application::open(&dir.path().join("library.sqlite")).unwrap();
        let v = AssetVersion {
            asset_id: "".into(),
            version_id: "v".into(),
            content_hash: "a".repeat(64),
            input_id: "input".into(),
            source: "source".into(),
            authorization: "declaration".into(),
            authorized: false,
            license_id: "license".into(),
            usage_scope: "test".into(),
            source_format: "glb".into(),
            contract_version: "1.0.0".into(),
            presence: Presence::Unknown,
        };
        let lease = storage::reference_guard(dir.path()).unwrap();
        assert_eq!(
            app.add_asset_version(&v),
            Err(LibraryError::InvalidInput("identity"))
        );
        let r = ComponentReference {
            owner_version_id: "".into(),
            role: ComponentRole::Mesh,
            component_version_id: "v".into(),
        };
        assert!(matches!(
            app.add_component_reference(&r),
            Err(LibraryError::Busy(_))
        ));
        drop(lease);
        assert_eq!(
            app.add_component_reference(&r),
            Err(LibraryError::InvalidInput("version identity"))
        );
        assert!(!dir.path().join(".aw-storage.lock").exists());
    }
    #[test]
    fn all_facade_error_categories_and_payloads_round_trip() {
        let errors = [
            LibraryError::Busy("busy detail".into()),
            LibraryError::Constraint("constraint detail".into()),
            LibraryError::Io("io detail".into()),
            LibraryError::Database("database detail".into()),
            LibraryError::FutureSchema(42),
            LibraryError::Uninitialized,
            LibraryError::InvalidInput("identity"),
            LibraryError::UnknownValue("future".into()),
        ];
        for error in errors {
            let mapped = LibraryError::from(MetadataError::from(error.clone()));
            assert_eq!(mapped, error);
            assert_eq!(mapped.to_string(), error.to_string());
        }
    }
    #[test]
    fn direct_port_writes_still_require_retention_lease() {
        let dir = tempfile::tempdir().unwrap();
        let (mut conn, _) =
            crate::sqlite::open_and_configure(&dir.path().join("library.sqlite")).unwrap();
        library::initialize(&mut conn).unwrap();
        let lease = storage::reference_guard(dir.path()).unwrap();
        let mut adapter = SqliteMetadata::new(&mut conn, dir.path());
        let r = ComponentReference {
            owner_version_id: "owner".into(),
            role: ComponentRole::Mesh,
            component_version_id: "component".into(),
        };
        assert!(matches!(
            adapter.register_reference(&r),
            Err(MetadataError::Busy(_))
        ));
        drop(lease);
        assert!(matches!(
            adapter.register_reference(&r),
            Err(MetadataError::Constraint(_))
        ));
        assert!(!dir.path().join(".aw-storage.lock").exists());
    }
}
