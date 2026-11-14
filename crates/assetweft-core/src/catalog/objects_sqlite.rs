//! Responsibility: concrete bounded CAS streaming and SQLite managed-object persistence.
//! Dependencies: Catalog semantic capability/policy, existing storage lease/binding, library schema guard.
//! Callers: storage compatibility composition; invariants: lease before IMMEDIATE, publish before registration.
use super::{
    domain::AssetVersion,
    objects_domain::{self as policy, Collection, ObjectPolicyError, StoredObject, hash_ok},
    objects_ports::ManagedObjects,
};
use crate::{
    library,
    storage::{StorageError, bind, lock},
};
use rusqlite::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
};

impl From<ObjectPolicyError> for StorageError {
    fn from(e: ObjectPolicyError) -> Self {
        match e {
            ObjectPolicyError::Metadata(e) => Self::Library(e.into()),
            ObjectPolicyError::Invalid(s) => Self::Invalid(s),
            ObjectPolicyError::Corrupt => Self::Corrupt,
        }
    }
}
pub(crate) struct SqliteObjects<'a, R> {
    conn: &'a mut Connection,
    root: &'a Path,
    reader: R,
}
impl<'a, R> SqliteObjects<'a, R> {
    pub(crate) fn new(conn: &'a mut Connection, root: &'a Path, reader: R) -> Self {
        Self { conn, root, reader }
    }
}
pub(crate) struct StagedObject {
    temp: tempfile::NamedTempFile,
    written: u64,
    dir: PathBuf,
}
fn objects(root: &Path) -> Result<PathBuf, StorageError> {
    let dir = root.join("objects");
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    if !fs::symlink_metadata(&dir)?.file_type().is_dir() {
        return Err(StorageError::Invalid("objects must be a real directory"));
    }
    Ok(dir)
}
fn digest(reader: &mut impl Read) -> Result<(String, u64), StorageError> {
    let mut h = Sha256::new();
    let mut n = 0u64;
    let mut buf = [0; 65536];
    loop {
        let count = reader.read(&mut buf)?;
        if count == 0 {
            break;
        }
        h.update(&buf[..count]);
        n += count as u64;
    }
    Ok((
        h.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
        n,
    ))
}
fn verify(path: &Path, hash: &str, size: u64) -> Result<(), StorageError> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(StorageError::Corrupt);
    }
    let mut file = File::open(path)?;
    if file.metadata()?.len() != size || digest(&mut file)? != (hash.into(), size) {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

impl<R: Read> ManagedObjects for SqliteObjects<'_, R> {
    type Error = StorageError;
    type Staged = StagedObject;
    fn stage(&mut self, v: &AssetVersion, expected_len: u64) -> Result<StagedObject, StorageError> {
        let conn = &mut *self.conn;
        let root = self.root;
        {
            let _guard = lock(root)?;
            bind(conn, root)?;
        }
        let dir = objects(root)?;
        let mut temp = tempfile::NamedTempFile::new_in(&dir)?;
        let mut h = Sha256::new();
        let mut written = 0u64;
        let mut buf = [0; 65536];
        // Read at most expected length + 1: catches both incomplete and excess input.
        let mut bounded = (&mut self.reader).take(expected_len + 1);
        loop {
            let n = bounded.read(&mut buf)?;
            if n == 0 {
                break;
            }
            written += n as u64;
            if written > expected_len {
                return Err(ObjectPolicyError::Invalid("excess input").into());
            }
            temp.write_all(&buf[..n])?;
            h.update(&buf[..n]);
        }
        policy::complete(written, expected_len)?;
        let actual = h
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        policy::matching_hash(&actual, &v.content_hash)?;
        temp.as_file().sync_all()?;
        temp.as_file_mut().rewind()?;
        if digest(temp.as_file_mut())? != (v.content_hash.clone(), written) {
            return Err(StorageError::Corrupt);
        }
        Ok(StagedObject { temp, written, dir })
    }
    fn publish(
        &mut self,
        v: &AssetVersion,
        staged: StagedObject,
    ) -> Result<StoredObject, StorageError> {
        let conn = &mut *self.conn;
        let root = self.root;
        let StagedObject { temp, written, dir } = staged;
        let _guard = lock(root)?;
        bind(conn, root)?;
        let target = dir.join(&v.content_hash);
        match fs::hard_link(temp.path(), &target) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        verify(&target, &v.content_hash, written)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        library::require_current(&tx)?;
        tx.execute("INSERT INTO aw_objects(content_hash,byte_length) VALUES(?1,?2) ON CONFLICT(content_hash) DO NOTHING",params![v.content_hash,written as i64])?;
        let size: i64 = tx.query_row(
            "SELECT byte_length FROM aw_objects WHERE content_hash=?1",
            [&v.content_hash],
            |r| r.get(0),
        )?;
        if size != written as i64 {
            return Err(StorageError::Corrupt);
        }
        crate::catalog::sqlite_metadata::insert_version(&tx, v)?;
        tx.execute(
            "INSERT INTO aw_managed_versions(version_id,content_hash) VALUES(?1,?2)",
            params![v.version_id, v.content_hash],
        )?;
        tx.commit()?;
        Ok(StoredObject {
            content_hash: v.content_hash.clone(),
            byte_length: written,
        })
    }
    fn managed(&mut self, id: &str) -> Result<Option<StoredObject>, StorageError> {
        let conn = &mut *self.conn;
        let root = self.root;
        use rusqlite::OptionalExtension;
        let _guard = lock(root)?;
        bind(conn, root)?;
        library::require_current(conn)?;
        let found=conn.query_row("SELECT o.content_hash,o.byte_length FROM aw_managed_versions v JOIN aw_objects o USING(content_hash) WHERE v.version_id=?1",[id],|r|Ok(StoredObject{content_hash:r.get(0)?,byte_length:r.get::<_,i64>(1)? as u64})).optional()?;
        if let Some(o) = &found {
            if !hash_ok(&o.content_hash) {
                return Err(StorageError::Corrupt);
            }
            verify(
                &objects(root)?.join(&o.content_hash),
                &o.content_hash,
                o.byte_length,
            )?;
        }
        Ok(found)
    }
    fn protect(
        &mut self,
        hash: &str,
        backup_pin: bool,
        unsynced_unique: bool,
    ) -> Result<(), StorageError> {
        let conn = &mut *self.conn;
        let root = self.root;
        let _guard = lock(root)?;
        bind(conn, root)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        library::require_current(&tx)?;
        let changed = tx.execute(
            "UPDATE aw_objects SET backup_pin=?2,unsynced_unique=?3 WHERE content_hash=?1",
            params![hash, backup_pin, unsynced_unique],
        )?;
        if changed != 1 {
            return Err(StorageError::Invalid("unknown managed object"));
        }
        tx.commit()?;
        Ok(())
    }
    fn collect(&mut self, hash: &str) -> Result<Collection, StorageError> {
        let conn = &mut *self.conn;
        let root = self.root;
        let _guard = lock(root)?;
        bind(conn, root)?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        library::require_current(&tx)?;
        let protected:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM aw_asset_versions WHERE content_hash=?1) OR EXISTS(SELECT 1 FROM aw_objects WHERE content_hash=?1 AND (backup_pin=1 OR unsynced_unique=1))",[hash],|r|r.get(0))?;
        if let Some(outcome) = policy::collection_policy(protected) {
            return Ok(outcome);
        }
        let path = objects(root)?.join(hash);
        // Validate before deleting a candidate; no symlink or wrong-content cleanup.
        match fs::symlink_metadata(&path) {
            Ok(meta) => verify(&path, hash, meta.len())?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        tx.execute("DELETE FROM aw_objects WHERE content_hash=?1", [hash])?;
        tx.commit()?; // Failure after this point leaves only an explainable file orphan.
        match fs::remove_file(path) {
            Ok(()) => Ok(Collection::Removed),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Collection::Absent),
            Err(e) => Err(e.into()),
        }
    }
}
