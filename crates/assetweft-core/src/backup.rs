//! Responsibility: legacy backup API and concrete composition boundary.
//! Dependencies: Catalog backup use cases/SQLite adapter and original diagnostic payloads.
//! Callers: Application and legacy consumers; invariants: identical public types and signatures.
pub use crate::catalog::backup_domain::{Limits, Manifest, Object, Progress};
use crate::{library, storage};
use std::path::Path;
#[derive(Debug)]
pub enum BackupError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    Library(library::LibraryError),
    Storage(storage::StorageError),
    Json(serde_json::Error),
    Invalid(&'static str),
    Busy,
    Cancelled,
    Limit,
}
impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BackupError {}
macro_rules! convert {
    ($t:ty,$v:ident) => {
        impl From<$t> for BackupError {
            fn from(e: $t) -> Self {
                Self::$v(e)
            }
        }
    };
}
convert!(std::io::Error, Io);
convert!(rusqlite::Error, Sql);
convert!(library::LibraryError, Library);
convert!(storage::StorageError, Storage);
convert!(serde_json::Error, Json);
impl From<crate::catalog::backup_domain::PolicyError> for BackupError {
    fn from(e: crate::catalog::backup_domain::PolicyError) -> Self {
        match e {
            crate::catalog::backup_domain::PolicyError::Invalid(s) => Self::Invalid(s),
            crate::catalog::backup_domain::PolicyError::Limit => Self::Limit,
        }
    }
}
/// Restores format 1/schema 6 only into a destination that must not exist.
/// # Errors
/// Rejects unsupported schemas, corruption, path aliases, limits, cancellation and IO.
pub fn restore(
    source: &Path,
    dest: &Path,
    limits: Limits,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<Manifest, BackupError> {
    let mut adapter =
        crate::catalog::backup_sqlite::RestoreSession::new(source, dest, limits, progress)?;
    crate::catalog::backup_application::restore(&mut adapter, limits)
}
