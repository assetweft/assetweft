use super::{
    domain::AssetVersion,
    objects_domain::{
        self as policy, Collection, FileAvailability, ObjectPolicyError, StoredObject, hash_ok,
    },
    objects_ports::ManagedObjects,
};
use crate::{
    library,
    storage::{StorageError, bind, check_binding, lock, redirected},
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
pub(super) fn verify(path: &Path, hash: &str, size: u64) -> Result<(), StorageError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || redirected(&metadata) {
        return Err(StorageError::Corrupt);
    }
    let mut file = File::open(path)?;
    if file.metadata()?.len() != size || digest(&mut file)? != (hash.into(), size) {
        return Err(StorageError::Corrupt);
    }
    Ok(())
}

pub(super) fn managed_record(
    conn: &Connection,
    id: &str,
) -> Result<Option<StoredObject>, StorageError> {
    use rusqlite::OptionalExtension;
    library::require_current(conn)?;
    let found = conn
        .query_row(
            "SELECT o.content_hash, o.byte_length, a.content_hash
         FROM aw_managed_versions v LEFT JOIN aw_objects o ON o.content_hash=v.content_hash
         LEFT JOIN aw_asset_versions a ON a.version_id=v.version_id WHERE v.version_id=?1",
            [id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    match found {
        Some((hash, size, expected)) if hash == expected && hash_ok(&hash) && size >= 0 => {
            Ok(Some(StoredObject {
                content_hash: hash,
                byte_length: size as u64,
            }))
        }
        Some(_) => Err(StorageError::Corrupt),
        None => Ok(None),
    }
}

pub(crate) fn file_availability(
    conn: &mut Connection,
    root: &Path,
    id: &str,
) -> Result<FileAvailability, StorageError> {
    if super::sqlite_metadata::asset_version(conn, id)?.is_none() {
        return Ok(FileAvailability::UnknownVersion);
    }
    let Some(object) = managed_record(conn, id)? else {
        return Ok(FileAvailability::MetadataOnly);
    };
    check_binding(conn, root)?;
    let directory = root.join("objects");
    let verify_managed = || {
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || redirected(&metadata) {
            return Err(StorageError::Invalid("objects must be a real directory"));
        }
        verify(
            &directory.join(&object.content_hash),
            &object.content_hash,
            object.byte_length,
        )
    };
    match verify_managed() {
        Ok(()) => Ok(FileAvailability::Available),
        Err(StorageError::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            Ok(FileAvailability::Missing)
        }
        Err(error) => Err(error),
    }
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
        let _guard = lock(root)?;
        bind(conn, root)?;
        let found = managed_record(conn, id)?;
        if let Some(object) = &found {
            verify(
                &objects(root)?.join(&object.content_hash),
                &object.content_hash,
                object.byte_length,
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
        match fs::symlink_metadata(&path) {
            Ok(meta) => verify(&path, hash, meta.len())?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        tx.execute("DELETE FROM aw_objects WHERE content_hash=?1", [hash])?;
        tx.commit()?;
        match fs::remove_file(path) {
            Ok(()) => Ok(Collection::Removed),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Collection::Absent),
            Err(e) => Err(e.into()),
        }
    }
}
