use crate::library::LibraryError;
use rusqlite::Connection;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub(crate) fn redirected(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Windows FILE_ATTRIBUTE_REPARSE_POINT also covers junctions.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.is_symlink()
    }
}

pub use crate::catalog::objects_domain::{Collection, FileAvailability, StoredObject};
#[derive(Debug)]
pub enum StorageError {
    Io(io::Error),
    Library(LibraryError),
    Invalid(&'static str),
    MissingPreviewSource,
    Corrupt,
    Busy,
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPreviewSource => f.write_str("managed source file is missing"),
            _ => write!(f, "{self:?}"),
        }
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

pub(crate) struct Guard(PathBuf, File);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.1.sync_all();
        let _ = fs::remove_file(&self.0);
    }
}
// Take the filesystem lease before SQLite's transaction lock (Catalog contract, managed objects).
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
pub(crate) fn reference_guard(root: &Path) -> Result<Guard, LibraryError> {
    match acquire(root) {
        Ok(guard) => Ok(guard),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            Err(LibraryError::Busy(e.to_string()))
        }
        Err(e) => Err(LibraryError::Io(e.to_string())),
    }
}
pub(crate) fn bind(conn: &Connection, root: &Path) -> Result<(), StorageError> {
    object_binding(conn, root, true)
}

pub(crate) fn check_binding(conn: &Connection, root: &Path) -> Result<(), StorageError> {
    object_binding(conn, root, false)
}

fn object_binding(conn: &Connection, root: &Path, create: bool) -> Result<(), StorageError> {
    let db =
        Path::new(conn.path().ok_or(StorageError::Invalid("file database"))?).canonicalize()?;
    let name = db
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(StorageError::Invalid("UTF-8 database filename"))?;
    let owner = root.join(".aw-object-owner");
    if create {
        match OpenOptions::new().write(true).create_new(true).open(&owner) {
            Ok(mut file) => {
                file.write_all(name.as_bytes())?;
                file.sync_all()?;
                return Ok(());
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    let metadata = fs::symlink_metadata(&owner)?;
    if !metadata.is_file() || redirected(&metadata) {
        return Err(StorageError::Invalid("store owner file"));
    }
    let mut bytes = Vec::new();
    File::open(&owner)?.take(4096).read_to_end(&mut bytes)?;
    if bytes != name.as_bytes() {
        return Err(StorageError::Invalid(
            "object directory belongs to another database",
        ));
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
