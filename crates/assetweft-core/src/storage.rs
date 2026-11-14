//! Responsibility: bounded managed CAS commits and explicit orphan collection.
//! Dependencies: sha2, tempfile, std filesystem and the library schema guard and Catalog borrowed insertion.
//! Callers: Application; filesystem is a private local library, not an adversarial shared tree.
//! Invariants: publish before DB, never replace objects, serialize GC with every hash-reference insertion and protection update.
//! A crash may leave a lock/temp/orphan; no automatic stale-lock breaking or power-loss claim.
use crate::library::{self, AssetVersion, LibraryError};
use rusqlite::{Connection, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
};

/// A verified managed object, independent of source/license metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub content_hash: String,
    pub byte_length: u64,
}
/// Explicit result of a requested single-object collection; no background deletion exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collection {
    Protected,
    Removed,
    Absent,
}
/// Preserve the original IO/database diagnostic; rejected input never registers a version.
#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Library(LibraryError),
    Invalid(&'static str),
    Corrupt,
    Busy,
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for StorageError {}
impl From<io::Error> for StorageError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<LibraryError> for StorageError {
    fn from(e: LibraryError) -> Self {
        Self::Library(e)
    }
}
impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Library(e.into())
    }
}

// Exclusive create establishes ownership. Drop removes only a successfully created lock.
pub(crate) struct Guard(PathBuf, File);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.1.sync_all();
        let _ = fs::remove_file(&self.0);
    }
}
// Both metadata-only references and managed operations use the same lease primitive.
// Global order: storage lease, then SQLite transaction. Never acquire it inside a tx.
fn acquire(root: &Path) -> io::Result<Guard> {
    let path = root.join(".aw-storage.lock");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map(|file| Guard(path, file))
}
fn lock(root: &Path) -> Result<Guard, StorageError> {
    match acquire(root) {
        Ok(guard) => Ok(guard),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(StorageError::Busy),
        Err(e) => Err(e.into()),
    }
}
/// Coordinates metadata hash references without requiring managed bytes or owner binding.
/// commit_object already holds this lease and uses the internal insert_version directly.
pub(crate) fn reference_guard(root: &Path) -> Result<Guard, LibraryError> {
    match acquire(root) {
        Ok(guard) => Ok(guard),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            Err(LibraryError::Busy(e.to_string()))
        }
        Err(e) => Err(LibraryError::Io(e.to_string())),
    }
}
// A directory may hold multiple SQLite files, but its objects belong to exactly one.
// Relative database filename keeps a closed whole-library move possible.
pub(crate) fn bind(conn: &Connection, root: &Path) -> Result<(), StorageError> {
    let db =
        Path::new(conn.path().ok_or(StorageError::Invalid("file database"))?).canonicalize()?;
    let name = db
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(StorageError::Invalid("UTF-8 database filename"))?;
    let owner = root.join(".aw-object-owner");
    match OpenOptions::new().write(true).create_new(true).open(&owner) {
        Ok(mut file) => {
            file.write_all(name.as_bytes())?;
            file.sync_all()?;
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            if !fs::symlink_metadata(&owner)?.file_type().is_file() {
                return Err(StorageError::Invalid("store owner file"));
            }
            let mut bytes = Vec::new();
            File::open(&owner)?.take(4096).read_to_end(&mut bytes)?;
            if bytes != name.as_bytes() {
                return Err(StorageError::Invalid(
                    "object directory belongs to another database",
                ));
            }
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
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
fn hash_ok(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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

pub(crate) fn commit(
    conn: &mut Connection,
    root: &Path,
    v: &AssetVersion,
    reader: &mut impl Read,
    expected_len: u64,
    max_bytes: u64,
) -> Result<StoredObject, StorageError> {
    library::validate(v)?;
    if expected_len > max_bytes || max_bytes > i64::MAX as u64 {
        return Err(StorageError::Invalid("byte limits"));
    }
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
    let mut bounded = reader.take(expected_len + 1);
    loop {
        let n = bounded.read(&mut buf)?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > expected_len {
            return Err(StorageError::Invalid("excess input"));
        }
        temp.write_all(&buf[..n])?;
        h.update(&buf[..n]);
    }
    if written != expected_len {
        return Err(StorageError::Invalid("incomplete input"));
    }
    if h.finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        != v.content_hash
    {
        return Err(StorageError::Corrupt);
    }
    temp.as_file().sync_all()?;
    temp.as_file_mut().rewind()?;
    if digest(temp.as_file_mut())? != (v.content_hash.clone(), written) {
        return Err(StorageError::Corrupt);
    }
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

pub(crate) fn managed(
    conn: &mut Connection,
    root: &Path,
    id: &str,
) -> Result<Option<StoredObject>, StorageError> {
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

pub(crate) fn protect(
    conn: &mut Connection,
    root: &Path,
    hash: &str,
    backup_pin: bool,
    unsynced_unique: bool,
) -> Result<(), StorageError> {
    if !hash_ok(hash) {
        return Err(StorageError::Invalid("hash"));
    }
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

pub(crate) fn collect(
    conn: &mut Connection,
    root: &Path,
    hash: &str,
) -> Result<Collection, StorageError> {
    if !hash_ok(hash) {
        return Err(StorageError::Invalid("hash"));
    }
    let _guard = lock(root)?;
    bind(conn, root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    library::require_current(&tx)?;
    let protected:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM aw_asset_versions WHERE content_hash=?1) OR EXISTS(SELECT 1 FROM aw_objects WHERE content_hash=?1 AND (backup_pin=1 OR unsynced_unique=1))",[hash],|r|r.get(0))?;
    if protected {
        return Ok(Collection::Protected);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_reference_lease_retains_io_error_type() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("not a directory");
        fs::write(&file, b"sentinel").unwrap();
        assert!(matches!(reference_guard(&file), Err(LibraryError::Io(_))));
        assert_eq!(fs::read(file).unwrap(), b"sentinel");
    }
}
