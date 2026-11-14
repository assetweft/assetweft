//! Responsibility: managed-object compatibility values/errors and shared concrete retention lease.
//! Dependencies: Catalog object use cases/adapter, library errors and std filesystem.
//! Callers: Application; filesystem is a private local library, not an adversarial shared tree.
//! Invariants: publish before DB, never replace objects, serialize GC with every hash-reference insertion and protection update.
//! A crash may leave a lock/temp/orphan; no automatic stale-lock breaking or power-loss claim.
use crate::library::LibraryError;
use rusqlite::Connection;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub use crate::catalog::objects_domain::{Collection, StoredObject};
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
pub(crate) fn lock(root: &Path) -> Result<Guard, StorageError> {
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
