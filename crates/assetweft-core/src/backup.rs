//! Responsibility: M1 consistent SQLite/CAS snapshots and exclusive fresh restore.
//! Dependencies: rusqlite Backup API, storage lease, library schema, serde and sha2.
//! Callers: Application only; reads registered objects from the snapshot, never live refs.
//! Invariants: lease spans snapshot/copy; manifest is published last; no existing target
//! is overwritten. Trusted local directories, not a sandbox against concurrent hostile Shell.
use crate::{library, storage};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions as FileOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

/// Operation limits. Deadline/cancellation are cooperative between bounded IO chunks;
/// callers must return promptly from callbacks. OS filesystem stalls are not preempted.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_bytes: u64,
    pub max_objects: usize,
    pub timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024 * 1024,
            max_objects: 100_000,
            timeout: Duration::from_secs(120),
        }
    }
}
/// Progress is also a cancellation boundary; returning false cancels without a manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Snapshot,
    Bytes(u64),
    ObjectsReady,
}
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub hash: String,
    pub bytes: u64,
}
/// Database digest binds all asset/version identities, references, history and metadata.
/// Object list includes all registered objects (also retained orphans), not external metadata.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub schema: i64,
    pub producer: String,
    pub database: Object,
    pub objects: Vec<Object>,
}
struct Run<'a> {
    limits: Limits,
    start: Instant,
    bytes: u64,
    progress: &'a mut dyn FnMut(Progress) -> bool,
}
impl Run<'_> {
    fn check(&mut self, progress: Progress) -> Result<(), BackupError> {
        if self.start.elapsed() >= self.limits.timeout {
            return Err(BackupError::Limit);
        }
        if !(self.progress)(progress) {
            return Err(BackupError::Cancelled);
        }
        Ok(())
    }
    fn stream(
        &mut self,
        path: &Path,
        mut target: Option<&mut File>,
    ) -> Result<Object, BackupError> {
        if !fs::symlink_metadata(path)?.file_type().is_file() {
            return Err(BackupError::Invalid("regular file required"));
        }
        let mut file = File::open(path)?;
        let mut hash = Sha256::new();
        let mut bytes = 0;
        let mut buf = [0; 65536];
        loop {
            self.check(Progress::Bytes(self.bytes))?;
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.bytes = self.bytes.checked_add(n as u64).ok_or(BackupError::Limit)?;
            if self.bytes > self.limits.max_bytes {
                return Err(BackupError::Limit);
            }
            bytes += n as u64;
            hash.update(&buf[..n]);
            if let Some(out) = target.as_mut() {
                out.write_all(&buf[..n])?;
            }
        }
        if let Some(out) = target {
            out.sync_all()?;
        }
        Ok(Object {
            hash: hash.finalize().iter().map(|b| format!("{b:02x}")).collect(),
            bytes,
        })
    }
}
fn safe_path(path: &Path) -> Result<(), BackupError> {
    let mut part = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(BackupError::Invalid("parent traversal"));
        }
        part.push(component);
        match fs::symlink_metadata(&part) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(BackupError::Invalid("symlink path"));
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn fresh(path: &Path) -> Result<(), BackupError> {
    safe_path(path)?;
    fs::create_dir(path)?;
    Ok(())
}
fn new_file(path: &Path) -> Result<File, BackupError> {
    Ok(FileOptions::new().write(true).create_new(true).open(path)?)
}
fn hash_ok(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn inspect(conn: &Connection, max: usize) -> Result<Vec<Object>, BackupError> {
    library::require_current(conn)?;
    let integrity: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if integrity != "ok"
        || conn
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .is_some()
    {
        return Err(BackupError::Invalid("database integrity"));
    }
    let mismatch:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM aw_managed_versions m JOIN aw_asset_versions v USING(version_id) WHERE m.content_hash!=v.content_hash)",[],|r|r.get(0))?;
    if mismatch {
        return Err(BackupError::Invalid("managed reference mismatch"));
    }
    let mut stmt =
        conn.prepare("SELECT content_hash,byte_length FROM aw_objects ORDER BY content_hash")?;
    let mut rows = stmt.query([])?;
    let mut objects = Vec::new();
    while let Some(row) = rows.next()? {
        if objects.len() >= max {
            return Err(BackupError::Limit);
        }
        let hash: String = row.get(0)?;
        let bytes: i64 = row.get(1)?;
        if !hash_ok(&hash) || bytes < 0 {
            return Err(BackupError::Invalid("object identity"));
        }
        objects.push(Object {
            hash,
            bytes: bytes as u64,
        });
    }
    Ok(objects)
}
fn copy_objects(
    source: &Path,
    dest: &Path,
    objects: &[Object],
    run: &mut Run<'_>,
) -> Result<(), BackupError> {
    fs::create_dir(dest)?;
    if !objects.is_empty() {
        safe_path(source)?;
    }
    for object in objects {
        if !hash_ok(&object.hash) {
            return Err(BackupError::Invalid("object name"));
        }
        let actual = run.stream(
            &source.join(&object.hash),
            Some(&mut new_file(&dest.join(&object.hash))?),
        )?;
        if actual != *object {
            return Err(BackupError::Invalid("object hash/length"));
        }
    }
    run.check(Progress::ObjectsReady)
}
pub(crate) fn create(
    conn: &Connection,
    root: &Path,
    dest: &Path,
    limits: Limits,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<Manifest, BackupError> {
    let mut run = Run {
        limits,
        start: Instant::now(),
        bytes: 0,
        progress,
    };
    let _guard = storage::reference_guard(root)?;
    storage::bind(conn, root)?;
    library::require_current(conn)?;
    fresh(dest)?;
    let db = dest.join("library.sqlite");
    new_file(&db)?;
    let mut snapshot = Connection::open(&db)?;
    snapshot.busy_timeout(Duration::ZERO)?;
    {
        let backup = Backup::new(conn, &mut snapshot)?;
        loop {
            run.check(Progress::Snapshot)?;
            match backup.step(128)? {
                StepResult::Done => break,
                StepResult::More => (),
                StepResult::Busy | StepResult::Locked => return Err(BackupError::Busy),
                _ => return Err(BackupError::Invalid("backup step")),
            }
            if u64::try_from(backup.progress().pagecount).unwrap_or(u64::MAX)
                > limits.max_bytes / 512
            {
                return Err(BackupError::Limit);
            }
        }
    }
    let objects = inspect(&snapshot, limits.max_objects)?;
    // Close/checkpoint before hashing: the snapshot package never depends on a sidecar.
    snapshot.close().map_err(|(_, e)| BackupError::Sql(e))?;
    let database = run.stream(&db, None)?;
    copy_objects(
        &root.join("objects"),
        &dest.join("objects"),
        &objects,
        &mut run,
    )?;
    let manifest = Manifest {
        format: 1,
        schema: library::CURRENT_SCHEMA,
        producer: env!("CARGO_PKG_VERSION").into(),
        database,
        objects,
    };
    publish_manifest(dest, &serde_json::to_vec(&manifest)?)?;
    Ok(manifest)
}
/// Restores format 1/schema 6 only; older and future schemas are rejected, never migrated.
/// Destination must not exist. Failure can leave an owned incomplete directory, never a
/// completion manifest. Source package is read-only; no stale lease/owner file is copied.
/// # Errors
/// Rejects unsupported schemas, corruption, path aliases, limits, cancellation and IO.
pub fn restore(
    source: &Path,
    dest: &Path,
    limits: Limits,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<Manifest, BackupError> {
    safe_path(source)?;
    safe_path(dest)?;
    let mut run = Run {
        limits,
        start: Instant::now(),
        bytes: 0,
        progress,
    };
    let manifest_path = source.join("manifest.json");
    if !fs::symlink_metadata(&manifest_path)?.file_type().is_file() {
        return Err(BackupError::Invalid("manifest file"));
    }
    let mut raw = Vec::new();
    File::open(manifest_path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut raw)?;
    if raw.len() > 16 * 1024 * 1024 {
        return Err(BackupError::Limit);
    }
    let manifest: Manifest = serde_json::from_slice(&raw)?;
    if manifest.format != 1
        || manifest.schema != library::CURRENT_SCHEMA
        || manifest.objects.len() > limits.max_objects
    {
        return Err(BackupError::Invalid("unsupported format/schema or count"));
    }
    let source_db = source.join("library.sqlite");
    if run.stream(&source_db, None)? != manifest.database {
        return Err(BackupError::Invalid("database hash/length"));
    }
    // Refuse sidecars: only a closed single-file snapshot is a valid package.
    for suffix in ["-wal", "-journal", "-shm"] {
        if fs::symlink_metadata(source.join(format!("library.sqlite{suffix}"))).is_ok() {
            return Err(BackupError::Invalid("snapshot sidecar"));
        }
    }
    let snapshot = Connection::open_with_flags(&source_db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if inspect(&snapshot, limits.max_objects)? != manifest.objects {
        return Err(BackupError::Invalid("snapshot object set"));
    }
    snapshot.close().map_err(|(_, e)| BackupError::Sql(e))?;
    fresh(dest)?;
    let db = dest.join("library.sqlite");
    if run.stream(&source_db, Some(&mut new_file(&db)?))? != manifest.database {
        return Err(BackupError::Invalid("snapshot changed"));
    }
    copy_objects(
        &source.join("objects"),
        &dest.join("objects"),
        &manifest.objects,
        &mut run,
    )?;
    // Bind the existing relative database filename contract without inheriting a lease.
    let mut owner = new_file(&dest.join(".aw-object-owner"))?;
    owner.write_all(b"library.sqlite")?;
    owner.sync_all()?;
    publish_manifest(dest, &raw)?;
    Ok(manifest)
}

// Atomic no-clobber completion marker: partial writes never look like a completed package.
fn publish_manifest(dest: &Path, bytes: &[u8]) -> Result<(), BackupError> {
    let mut temp = tempfile::NamedTempFile::new_in(dest)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(dest.join("manifest.json"))
        .map_err(|e| BackupError::Io(e.error))?;
    Ok(())
}
